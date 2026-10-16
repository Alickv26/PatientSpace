//! SQLite-backed PSEP event log.
//!
//! Implements the storage layer for PSEP envelopes, vector clock state, and
//! the causal DAG. Specified in `docs/sync-protocol.md` §2.6 and §10.
//!
//! # Invariants
//!
//! - The `envelopes` table is append-only. No UPDATE, no DELETE in app code.
//! - Inserting the same `version_id` twice is a no-op (idempotent ingest).
//! - The `vector_clocks` table is a *cache* of the highest counter observed
//!   per (resource, facility) pair, derivable from `envelopes`. Kept for
//!   fast sync handshakes.
//! - The `causal_edges` table stores the parent links of the version DAG.

use std::path::Path;
use std::str::FromStr;

use sqlx::sqlite::{SqliteConnectOptions, SqlitePool, SqlitePoolOptions};
use sqlx::Row;
use uuid::Uuid;

use patientspace_shared::{FacilityId, ResourceId, VersionId};

use crate::envelope::Envelope;
use crate::vector_clock::VectorClock;

/// Errors that can arise from the log layer.
#[derive(Debug, thiserror::Error)]
pub enum LogError {
    #[error("database error: {0}")]
    Db(#[from] sqlx::Error),

    #[error("migration error: {0}")]
    Migrate(#[from] sqlx::migrate::MigrateError),

    #[error("envelope already ingested: version_id={0}")]
    Duplicate(VersionId),

    #[error("not found: {0}")]
    NotFound(String),

    #[error("invalid envelope: {0}")]
    Invalid(String),
}

pub type LogResult<T> = Result<T, LogError>;

/// A handle to the local PSEP event log.
///
/// Cheaply clonable — wraps a `SqlitePool` internally.
#[derive(Clone)]
pub struct EventLog {
    pool: SqlitePool,
}

impl EventLog {
    /// Get a reference to the underlying SQLite pool (for auth tables, etc.).
    pub fn pool(&self) -> &SqlitePool {
        &self.pool
    }

    /// Open an event log at the given SQLite URL.
    ///
    /// Example URLs:
    /// - `sqlite:patientspace.db?mode=rwc` — file-backed, create if missing
    /// - `sqlite::memory:` — in-memory (tests only; data lost on close)
    /// - `sqlite:file::memory:?cache=shared` — shared in-memory
    pub async fn open(database_url: &str) -> LogResult<Self> {
        let opts = SqliteConnectOptions::from_str(database_url)?
            .create_if_missing(true)
            .journal_mode(sqlx::sqlite::SqliteJournalMode::Wal)
            .synchronous(sqlx::sqlite::SqliteSynchronous::Normal)
            .busy_timeout(std::time::Duration::from_secs(5))
            .foreign_keys(true);

        let pool = SqlitePoolOptions::new()
            .max_connections(5)
            .connect_with(opts)
            .await?;

        let log = Self { pool };
        log.run_migrations().await?;
        Ok(log)
    }

    /// Open an event log backed by a file path. Convenience wrapper around `open`.
    pub async fn open_file<P: AsRef<Path>>(path: P) -> LogResult<Self> {
        let url = format!("sqlite://{}?mode=rwc", path.as_ref().display());
        Self::open(&url).await
    }

    /// Open an in-memory event log. Useful for tests.
    pub async fn open_in_memory() -> LogResult<Self> {
        Self::open("sqlite::memory:").await
    }

    async fn run_migrations(&self) -> LogResult<()> {
        sqlx::migrate!("./migrations").run(&self.pool).await?;
        Ok(())
    }

    /// Close the connection pool. Optional — drops happen automatically on Drop.
    pub async fn close(&self) {
        self.pool.close().await;
    }

    // -----------------------------------------------------------------
    // Envelope ingest
    // -----------------------------------------------------------------

    /// Ingest a single envelope into the log.
    ///
    /// Idempotent: ingesting the same `version_id` twice is a no-op (returns
    /// `Ok(())` without modifying any tables).
    ///
    /// Updates the `vector_clocks` cache (component-wise max) and inserts
    /// rows into `causal_edges` for each parent in `parent_version_ids`.
    ///
    /// Does NOT verify the envelope signature — that's the caller's
    /// responsibility (the log is a dumb store).
    pub async fn ingest(&self, envelope: &Envelope) -> LogResult<()> {
        let version_id = envelope.version_id;
        let resource_id = envelope.resource_id;
        let facility_id = &envelope.author_facility_id;
        let resource_type = &envelope.resource_type;
        let created_at = envelope.created_at_unix_ms as i64;
        let ingested_at = now_unix_ms() as i64;

        let envelope_blob = envelope.to_cbor().map_err(|e| {
            LogError::Invalid(format!("envelope CBOR encode failed: {e}"))
        })?;

        let mut tx = self.pool.begin().await?;

        // 1. Insert the envelope row. ON CONFLICT DO NOTHING = idempotent.
        let inserted = sqlx::query(
            r#"
            INSERT OR IGNORE INTO envelopes
                (facility_id, version_id, resource_id, resource_type,
                 envelope_blob, created_at_unix_ms, ingested_at_unix_ms)
            VALUES (?, ?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(facility_id)
        .bind(version_id.to_bytes_le().to_vec())
        .bind(resource_id.to_bytes_le().to_vec())
        .bind(resource_type)
        .bind(&envelope_blob)
        .bind(created_at)
        .bind(ingested_at)
        .execute(&mut *tx)
        .await?;

        if inserted.rows_affected() == 0 {
            // Already present — no-op.
            tx.rollback().await?;
            return Ok(());
        }

        // 2. Update the vector clock cache for this resource.
        for (vc_facility, vc_counter) in &envelope.vector_clock.0 {
            sqlx::query(
                r#"
                INSERT INTO vector_clocks (resource_id, facility_id, counter)
                VALUES (?, ?, ?)
                ON CONFLICT (resource_id, facility_id) DO UPDATE
                  SET counter = MAX(excluded.counter, vector_clocks.counter)
                "#,
            )
            .bind(resource_id.to_bytes_le().to_vec())
            .bind(vc_facility)
            .bind(*vc_counter as i64)
            .execute(&mut *tx)
            .await?;
        }

        // 3. Insert causal edges.
        for parent in &envelope.parent_version_ids {
            sqlx::query(
                r#"INSERT OR IGNORE INTO causal_edges (version_id, parent_version_id)
                   VALUES (?, ?)"#,
            )
            .bind(version_id.to_bytes_le().to_vec())
            .bind(parent.to_bytes_le().to_vec())
            .execute(&mut *tx)
            .await?;
        }

        tx.commit().await?;
        Ok(())
    }

    // -----------------------------------------------------------------
    // Envelope queries
    // -----------------------------------------------------------------

    /// Fetch a single envelope by version_id. Returns `NotFound` if absent.
    pub async fn get(&self, version_id: &VersionId) -> LogResult<Envelope> {
        let row = sqlx::query(
            r#"SELECT envelope_blob FROM envelopes WHERE version_id = ?"#,
        )
        .bind(version_id.0.to_bytes_le().to_vec())
        .fetch_optional(&self.pool)
        .await?;

        let row = row.ok_or_else(|| {
            LogError::NotFound(format!("version_id={version_id}"))
        })?;

        let blob: Vec<u8> = row.try_get("envelope_blob")?;
        Envelope::from_cbor(&blob).map_err(|e| {
            LogError::Invalid(format!("envelope CBOR decode failed: {e}"))
        })
    }

    /// Fetch all versions of a resource, newest first by `created_at_unix_ms`.
    /// Returns an empty vec if no versions exist.
    pub async fn list_by_resource(
        &self,
        resource_id: &ResourceId,
    ) -> LogResult<Vec<Envelope>> {
        let rows = sqlx::query(
            r#"SELECT envelope_blob FROM envelopes
               WHERE resource_id = ?
               ORDER BY created_at_unix_ms DESC"#,
        )
        .bind(resource_id.0.to_bytes_le().to_vec())
        .fetch_all(&self.pool)
        .await?;

        decode_rows(rows)
    }

    /// Fetch the latest version of a resource. Returns `NotFound` if no
    /// versions exist. "Latest" = highest `created_at_unix_ms`; ties broken
    /// by lexicographic `version_id` (deterministic).
    pub async fn latest_for_resource(
        &self,
        resource_id: &ResourceId,
    ) -> LogResult<Envelope> {
        let row = sqlx::query(
            r#"SELECT envelope_blob FROM envelopes
               WHERE resource_id = ?
               ORDER BY created_at_unix_ms DESC, version_id DESC
               LIMIT 1"#,
        )
        .bind(resource_id.0.to_bytes_le().to_vec())
        .fetch_optional(&self.pool)
        .await?;

        let row = row.ok_or_else(|| {
            LogError::NotFound(format!("resource_id={resource_id}"))
        })?;

        let blob: Vec<u8> = row.try_get("envelope_blob")?;
        Envelope::from_cbor(&blob).map_err(|e| {
            LogError::Invalid(format!("envelope CBOR decode failed: {e}"))
        })
    }

    /// Fetch all envelopes of a given FHIR resource type, newest first.
    /// Used by FHIR `_type` searches.
    pub async fn list_by_type(
        &self,
        resource_type: &str,
        limit: u32,
    ) -> LogResult<Vec<Envelope>> {
        let rows = sqlx::query(
            r#"SELECT envelope_blob FROM envelopes
               WHERE resource_type = ?
               ORDER BY created_at_unix_ms DESC
               LIMIT ?"#,
        )
        .bind(resource_type)
        .bind(limit as i64)
        .fetch_all(&self.pool)
        .await?;

        decode_rows(rows)
    }

    /// Fetch all envelopes authored by a given facility, in ingest order.
    /// Used by the sync agent's push phase.
    pub async fn list_by_facility(
        &self,
        facility_id: &FacilityId,
        since_ingested_unix_ms: u64,
        limit: u32,
    ) -> LogResult<Vec<Envelope>> {
        let rows = sqlx::query(
            r#"SELECT envelope_blob FROM envelopes
               WHERE facility_id = ?
                 AND ingested_at_unix_ms > ?
               ORDER BY ingested_at_unix_ms ASC
               LIMIT ?"#,
        )
        .bind(facility_id.as_str())
        .bind(since_ingested_unix_ms as i64)
        .bind(limit as i64)
        .fetch_all(&self.pool)
        .await?;

        decode_rows(rows)
    }

    /// Count total envelopes in the log. Used for diagnostics + the
    /// migration validation report.
    pub async fn count(&self) -> LogResult<u64> {
        let row = sqlx::query(r#"SELECT COUNT(*) AS n FROM envelopes"#)
            .fetch_one(&self.pool)
            .await?;
        let n: i64 = row.try_get("n")?;
        Ok(n as u64)
    }

    /// Fetch all envelopes in the log, newest first by `created_at_unix_ms`.
    /// Used by the hub's pull endpoint to enumerate every version it holds
    /// (the hub then filters by the requesting facility's vector clock).
    pub async fn list_all(&self, limit: u32) -> LogResult<Vec<Envelope>> {
        let rows = sqlx::query(
            r#"SELECT envelope_blob FROM envelopes
               ORDER BY created_at_unix_ms DESC
               LIMIT ?"#,
        )
        .bind(limit as i64)
        .fetch_all(&self.pool)
        .await?;

        decode_rows(rows)
    }

    // -----------------------------------------------------------------
    // Vector clock queries
    // -----------------------------------------------------------------

    /// Get the merged vector clock for a resource — the component-wise max
    /// of every version's vector clock that we've ingested for this resource.
    ///
    /// Equivalent to walking `list_by_resource` and merging each version's
    /// clock, but served from the `vector_clocks` cache (one indexed query).
    pub async fn vector_clock_for_resource(
        &self,
        resource_id: &ResourceId,
    ) -> LogResult<VectorClock> {
        let rows = sqlx::query(
            r#"SELECT facility_id, counter FROM vector_clocks
               WHERE resource_id = ?"#,
        )
        .bind(resource_id.0.to_bytes_le().to_vec())
        .fetch_all(&self.pool)
        .await?;

        let mut vc = VectorClock::new();
        for row in rows {
            let facility_id: String = row.try_get("facility_id")?;
            let counter: i64 = row.try_get("counter")?;
            vc.observe(&FacilityId::new(facility_id), counter as u64);
        }
        Ok(vc)
    }

    /// Get the global vector clock — component-wise max across all resources.
    /// Used by the sync handshake (`facility_clock` field in §7.2).
    pub async fn global_vector_clock(&self) -> LogResult<VectorClock> {
        let rows = sqlx::query(
            r#"SELECT facility_id, MAX(counter) AS max_counter
               FROM vector_clocks
               GROUP BY facility_id"#,
        )
        .fetch_all(&self.pool)
        .await?;

        let mut vc = VectorClock::new();
        for row in rows {
            let facility_id: String = row.try_get("facility_id")?;
            let max_counter: i64 = row.try_get("max_counter")?;
            vc.observe(&FacilityId::new(facility_id), max_counter as u64);
        }
        Ok(vc)
    }

    // -----------------------------------------------------------------
    // Causal DAG queries
    // -----------------------------------------------------------------

    /// Get the parent version IDs of a given version.
    /// Genesis versions return an empty vec.
    pub async fn parents(&self, version_id: &VersionId) -> LogResult<Vec<VersionId>> {
        let rows = sqlx::query(
            r#"SELECT parent_version_id FROM causal_edges
               WHERE version_id = ?"#,
        )
        .bind(version_id.0.to_bytes_le().to_vec())
        .fetch_all(&self.pool)
        .await?;

        let mut parents = Vec::with_capacity(rows.len());
        for row in rows {
            let bytes: Vec<u8> = row.try_get("parent_version_id")?;
            let uuid = Uuid::from_bytes_le(
                bytes.as_slice().try_into().map_err(|_| {
                    LogError::Invalid(format!(
                        "parent_version_id has wrong length: {} bytes",
                        bytes.len()
                    ))
                })?,
            );
            parents.push(VersionId(uuid));
        }
        Ok(parents)
    }

    /// Get the child version IDs of a given version (forward DAG traversal).
    pub async fn children(&self, version_id: &VersionId) -> LogResult<Vec<VersionId>> {
        let rows = sqlx::query(
            r#"SELECT version_id FROM causal_edges
               WHERE parent_version_id = ?"#,
        )
        .bind(version_id.0.to_bytes_le().to_vec())
        .fetch_all(&self.pool)
        .await?;

        let mut children = Vec::with_capacity(rows.len());
        for row in rows {
            let bytes: Vec<u8> = row.try_get("version_id")?;
            let uuid = Uuid::from_bytes_le(
                bytes.as_slice().try_into().map_err(|_| {
                    LogError::Invalid(format!(
                        "version_id has wrong length: {} bytes",
                        bytes.len()
                    ))
                })?,
            );
            children.push(VersionId(uuid));
        }
        Ok(children)
    }

    // -----------------------------------------------------------------
    // Sync state (watermarks, session metadata)
    // -----------------------------------------------------------------

    /// Get a sync-state value for a facility. Returns `None` if the key
    /// has never been set. Used by the sync agent to track watermarks like
    /// `last_push_ingested_ms`.
    pub async fn get_sync_state(
        &self,
        facility_id: &FacilityId,
        key: &str,
    ) -> LogResult<Option<Vec<u8>>> {
        let row = sqlx::query(
            r#"SELECT value FROM sync_state WHERE facility_id = ? AND key = ?"#,
        )
        .bind(facility_id.as_str())
        .bind(key)
        .fetch_optional(&self.pool)
        .await?;

        match row {
            Some(row) => {
                let value: Vec<u8> = row.try_get("value")?;
                Ok(Some(value))
            }
            None => Ok(None),
        }
    }

    /// Set a sync-state value for a facility. Idempotent (upserts).
    pub async fn set_sync_state(
        &self,
        facility_id: &FacilityId,
        key: &str,
        value: &[u8],
    ) -> LogResult<()> {
        sqlx::query(
            r#"INSERT INTO sync_state (facility_id, key, value)
               VALUES (?, ?, ?)
               ON CONFLICT (facility_id, key) DO UPDATE
                 SET value = excluded.value"#,
        )
        .bind(facility_id.as_str())
        .bind(key)
        .bind(value)
        .execute(&self.pool)
        .await?;
        Ok(())
    }
}

// -----------------------------------------------------------------
// Helpers
// -----------------------------------------------------------------

fn decode_rows(rows: Vec<sqlx::sqlite::SqliteRow>) -> LogResult<Vec<Envelope>> {
    let mut out = Vec::with_capacity(rows.len());
    for row in rows {
        let blob: Vec<u8> = row.try_get("envelope_blob")?;
        let env = Envelope::from_cbor(&blob).map_err(|e| {
            LogError::Invalid(format!("envelope CBOR decode failed: {e}"))
        })?;
        out.push(env);
    }
    Ok(out)
}

fn now_unix_ms() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

// -----------------------------------------------------------------
// Tests
// -----------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use patientspace_shared::{ClinicianId, FacilityId, ResourceId, ResourceKind, VersionId};
    use crate::envelope::EnvelopeBuilder;
    use crate::keys::FacilityKeypair;
    use crate::vector_clock::VectorClock;

    /// Build a test envelope with a random keypair and the given vector clock.
    fn make_envelope(
        keypair: &FacilityKeypair,
        facility_id: &FacilityId,
        resource_id: &ResourceId,
        version_id: &VersionId,
        vector_clock: VectorClock,
        parents: Vec<VersionId>,
    ) -> Envelope {
        let plaintext = br#"{"resourceType":"Patient","id":"U-test"}"#.to_vec();
        let author_pub = keypair.public_keys();
        let builder = EnvelopeBuilder {
            version_id: version_id.clone(),
            resource_id: resource_id.clone(),
            resource_type: "Patient".to_string(),
            vector_clock,
            parent_version_ids: parents,
            author_facility_id: facility_id.clone(),
            author_clinician_id: ClinicianId::new(),
            created_at_unix_ms: 1_729_000_000_000,
            kind: ResourceKind::Clinical,
            plaintext,
        };
        let recipients: Vec<(&FacilityId, &crate::keys::FacilityPublicKeys)> =
            vec![(facility_id, &author_pub)];
        builder.seal(keypair, &recipients).expect("seal")
    }

    #[tokio::test]
    async fn open_in_memory_runs_migrations() {
        let log = EventLog::open_in_memory().await.expect("open");
        // Migrations should have created the envelopes table.
        let count = log.count().await.expect("count");
        assert_eq!(count, 0);
    }

    #[tokio::test]
    async fn ingest_and_get_single_envelope() {
        let log = EventLog::open_in_memory().await.unwrap();
        let kp = FacilityKeypair::generate();
        let fid = FacilityId::new("F-PETAUKE1");
        let rid = ResourceId::new();
        let vid = VersionId::new();
        let vc = VectorClock::from_facility(&fid, 1);
        let env = make_envelope(&kp, &fid, &rid, &vid, vc, vec![]);

        log.ingest(&env).await.expect("ingest");

        let back = log.get(&vid).await.expect("get");
        assert_eq!(back.version_id, vid.0);
        assert_eq!(back.resource_id, rid.0);
        assert_eq!(back.author_facility_id, "F-PETAUKE1");
    }

    #[tokio::test]
    async fn ingest_is_idempotent() {
        let log = EventLog::open_in_memory().await.unwrap();
        let kp = FacilityKeypair::generate();
        let fid = FacilityId::new("F-PETAUKE1");
        let rid = ResourceId::new();
        let vid = VersionId::new();
        let env = make_envelope(&kp, &fid, &rid, &vid, VectorClock::from_facility(&fid, 1), vec![]);

        log.ingest(&env).await.unwrap();
        log.ingest(&env).await.unwrap();
        log.ingest(&env).await.unwrap();

        assert_eq!(log.count().await.unwrap(), 1);
    }

    #[tokio::test]
    async fn list_by_resource_returns_newest_first() {
        let log = EventLog::open_in_memory().await.unwrap();
        let kp = FacilityKeypair::generate();
        let fid = FacilityId::new("F-PETAUKE1");
        let rid = ResourceId::new();

        // Three versions with strictly increasing created_at timestamps.
        for ts in [1_700_000_000_000u64, 1_710_000_000_000, 1_720_000_000_000] {
            let vid = VersionId::new();
            let plaintext = br#"{"resourceType":"Patient"}"#.to_vec();
            let author_pub = kp.public_keys();
            let builder = EnvelopeBuilder {
                version_id: vid,
                resource_id: rid.clone(),
                resource_type: "Patient".to_string(),
                vector_clock: VectorClock::from_facility(&fid, 1),
                parent_version_ids: vec![],
                author_facility_id: fid.clone(),
                author_clinician_id: ClinicianId::new(),
                created_at_unix_ms: ts,
                kind: ResourceKind::Clinical,
                plaintext,
            };
            let env = builder.seal(&kp, &[(&fid, &author_pub)]).unwrap();
            log.ingest(&env).await.unwrap();
        }

        let versions = log.list_by_resource(&rid).await.unwrap();
        assert_eq!(versions.len(), 3);
        // Newest first.
        assert_eq!(versions[0].created_at_unix_ms, 1_720_000_000_000);
        assert_eq!(versions[1].created_at_unix_ms, 1_710_000_000_000);
        assert_eq!(versions[2].created_at_unix_ms, 1_700_000_000_000);
    }

    #[tokio::test]
    async fn latest_for_resource() {
        let log = EventLog::open_in_memory().await.unwrap();
        let kp = FacilityKeypair::generate();
        let fid = FacilityId::new("F-PETAUKE1");
        let rid = ResourceId::new();

        for ts in [1_700_000_000_000u64, 1_720_000_000_000, 1_710_000_000_000] {
            let vid = VersionId::new();
            let plaintext = br#"{"resourceType":"Patient"}"#.to_vec();
            let author_pub = kp.public_keys();
            let builder = EnvelopeBuilder {
                version_id: vid,
                resource_id: rid.clone(),
                resource_type: "Patient".to_string(),
                vector_clock: VectorClock::from_facility(&fid, 1),
                parent_version_ids: vec![],
                author_facility_id: fid.clone(),
                author_clinician_id: ClinicianId::new(),
                created_at_unix_ms: ts,
                kind: ResourceKind::Clinical,
                plaintext,
            };
            let env = builder.seal(&kp, &[(&fid, &author_pub)]).unwrap();
            log.ingest(&env).await.unwrap();
        }

        let latest = log.latest_for_resource(&rid).await.unwrap();
        assert_eq!(latest.created_at_unix_ms, 1_720_000_000_000);
    }

    #[tokio::test]
    async fn latest_for_resource_returns_not_found_when_empty() {
        let log = EventLog::open_in_memory().await.unwrap();
        let rid = ResourceId::new();
        let err = log.latest_for_resource(&rid).await.unwrap_err();
        assert!(matches!(err, LogError::NotFound(_)));
    }

    #[tokio::test]
    async fn list_by_type() {
        let log = EventLog::open_in_memory().await.unwrap();
        let kp = FacilityKeypair::generate();
        let fid = FacilityId::new("F-PETAUKE1");

        // 2 Patients + 1 Encounter.
        for resource_type in ["Patient", "Patient", "Encounter"] {
            let vid = VersionId::new();
            let rid = ResourceId::new();
            let plaintext = format!(r#"{{"resourceType":"{resource_type}"}}"#).into_bytes();
            let author_pub = kp.public_keys();
            let builder = EnvelopeBuilder {
                version_id: vid,
                resource_id: rid,
                resource_type: resource_type.to_string(),
                vector_clock: VectorClock::from_facility(&fid, 1),
                parent_version_ids: vec![],
                author_facility_id: fid.clone(),
                author_clinician_id: ClinicianId::new(),
                created_at_unix_ms: 1_700_000_000_000,
                kind: ResourceKind::Clinical,
                plaintext,
            };
            let env = builder.seal(&kp, &[(&fid, &author_pub)]).unwrap();
            log.ingest(&env).await.unwrap();
        }

        let patients = log.list_by_type("Patient", 100).await.unwrap();
        assert_eq!(patients.len(), 2);
        let encounters = log.list_by_type("Encounter", 100).await.unwrap();
        assert_eq!(encounters.len(), 1);
        let observations = log.list_by_type("Observation", 100).await.unwrap();
        assert_eq!(observations.len(), 0);
    }

    #[tokio::test]
    async fn list_by_facility() {
        let log = EventLog::open_in_memory().await.unwrap();
        let kp_a = FacilityKeypair::generate();
        let kp_b = FacilityKeypair::generate();
        let fid_a = FacilityId::new("F-PETAUKE1");
        let fid_b = FacilityId::new("F-LUSAKA2");

        for fid_kp in [(&fid_a, &kp_a), (&fid_b, &kp_b)] {
            let (fid, kp) = fid_kp;
            for _ in 0..3 {
                let vid = VersionId::new();
                let rid = ResourceId::new();
                let plaintext = br#"{"resourceType":"Patient"}"#.to_vec();
                let author_pub = kp.public_keys();
                let builder = EnvelopeBuilder {
                    version_id: vid,
                    resource_id: rid,
                    resource_type: "Patient".to_string(),
                    vector_clock: VectorClock::from_facility(fid, 1),
                    parent_version_ids: vec![],
                    author_facility_id: fid.clone(),
                    author_clinician_id: ClinicianId::new(),
                    created_at_unix_ms: 1_700_000_000_000,
                    kind: ResourceKind::Clinical,
                    plaintext,
                };
                let env = builder.seal(kp, &[(&fid, &author_pub)]).unwrap();
                log.ingest(&env).await.unwrap();
            }
        }

        let a_envs = log.list_by_facility(&fid_a, 0, 100).await.unwrap();
        assert_eq!(a_envs.len(), 3);
        for env in &a_envs {
            assert_eq!(env.author_facility_id, "F-PETAUKE1");
        }

        let b_envs = log.list_by_facility(&fid_b, 0, 100).await.unwrap();
        assert_eq!(b_envs.len(), 3);
    }

    #[tokio::test]
    async fn vector_clock_for_resource_sums_all_facilities() {
        let log = EventLog::open_in_memory().await.unwrap();
        let kp = FacilityKeypair::generate();
        let fid_a = FacilityId::new("F-A");
        let fid_b = FacilityId::new("F-B");
        let rid = ResourceId::new();

        // Version 1: VC = {A: 1}
        let env1 = make_envelope(
            &kp,
            &fid_a,
            &rid,
            &VersionId::new(),
            VectorClock::from_facility(&fid_a, 1),
            vec![],
        );
        log.ingest(&env1).await.unwrap();

        // Version 2: VC = {A: 1, B: 3}  (B has observed up to A:1 and itself:3)
        let mut vc2 = VectorClock::from_facility(&fid_a, 1);
        vc2.observe(&fid_b, 3);
        let env2 = make_envelope(&kp, &fid_b, &rid, &VersionId::new(), vc2, vec![]);
        log.ingest(&env2).await.unwrap();

        let merged = log.vector_clock_for_resource(&rid).await.unwrap();
        assert_eq!(merged.get(&fid_a), 1);
        assert_eq!(merged.get(&fid_b), 3);
    }

    #[tokio::test]
    async fn global_vector_clock_is_max_across_resources() {
        let log = EventLog::open_in_memory().await.unwrap();
        let kp = FacilityKeypair::generate();
        let fid_a = FacilityId::new("F-A");
        let fid_b = FacilityId::new("F-B");

        // Resource 1: A observed at counter 5.
        let env1 = make_envelope(
            &kp,
            &fid_a,
            &ResourceId::new(),
            &VersionId::new(),
            VectorClock::from_facility(&fid_a, 5),
            vec![],
        );
        log.ingest(&env1).await.unwrap();

        // Resource 2: A observed at counter 3 (lower), B observed at counter 7.
        let mut vc2 = VectorClock::new();
        vc2.observe(&fid_a, 3);
        vc2.observe(&fid_b, 7);
        let env2 = make_envelope(&kp, &fid_b, &ResourceId::new(), &VersionId::new(), vc2, vec![]);
        log.ingest(&env2).await.unwrap();

        let g = log.global_vector_clock().await.unwrap();
        assert_eq!(g.get(&fid_a), 5); // max across resources
        assert_eq!(g.get(&fid_b), 7);
    }

    #[tokio::test]
    async fn causal_edges_parents_and_children() {
        let log = EventLog::open_in_memory().await.unwrap();
        let kp = FacilityKeypair::generate();
        let fid = FacilityId::new("F-A");
        let rid = ResourceId::new();

        // Genesis: no parents.
        let v0 = VersionId::new();
        let env0 = make_envelope(&kp, &fid, &rid, &v0, VectorClock::from_facility(&fid, 1), vec![]);
        log.ingest(&env0).await.unwrap();
        assert_eq!(log.parents(&v0).await.unwrap().len(), 0);

        // Linear edit: parent = v0.
        let v1 = VersionId::new();
        let vc1 = VectorClock::from_facility(&fid, 2);
        let env1 = make_envelope(&kp, &fid, &rid, &v1, vc1.clone(), vec![v0.clone()]);
        log.ingest(&env1).await.unwrap();
        let parents = log.parents(&v1).await.unwrap();
        assert_eq!(parents.len(), 1);
        assert_eq!(parents[0], v0);

        // v0 should now have v1 as a child.
        let children = log.children(&v0).await.unwrap();
        assert_eq!(children.len(), 1);
        assert_eq!(children[0], v1);

        // Merge version: parents = [v1, v2].
        let v2 = VersionId::new();
        let env2 = make_envelope(
            &kp,
            &fid,
            &rid,
            &v2,
            VectorClock::from_facility(&fid, 3),
            vec![v0.clone()],
        );
        log.ingest(&env2).await.unwrap();

        let v_merge = VersionId::new();
        let mut vc_merge = VectorClock::from_facility(&fid, 4);
        vc_merge.observe(&fid, 4);
        let env_merge = make_envelope(&kp, &fid, &rid, &v_merge, vc_merge, vec![v1.clone(), v2.clone()]);
        log.ingest(&env_merge).await.unwrap();

        let merge_parents = log.parents(&v_merge).await.unwrap();
        assert_eq!(merge_parents.len(), 2);
        assert!(merge_parents.contains(&v1));
        assert!(merge_parents.contains(&v2));
    }

    #[tokio::test]
    async fn open_file_creates_db_and_runs_migrations() {
        let tmp = tempfile::NamedTempFile::new().unwrap();
        let path = tmp.path().to_str().unwrap();
        let url = format!("sqlite://{path}?mode=rwc");

        let log = EventLog::open(&url).await.expect("open");
        assert_eq!(log.count().await.unwrap(), 0);

        let kp = FacilityKeypair::generate();
        let fid = FacilityId::new("F-FILE");
        let rid = ResourceId::new();
        let vid = VersionId::new();
        let env = make_envelope(&kp, &fid, &rid, &vid, VectorClock::from_facility(&fid, 1), vec![]);
        log.ingest(&env).await.unwrap();
        assert_eq!(log.count().await.unwrap(), 1);
        log.close().await;

        // Reopen — data should persist and migration should be a no-op.
        let log2 = EventLog::open(&url).await.expect("reopen");
        assert_eq!(log2.count().await.unwrap(), 1);
        let _ = log2.get(&vid).await.expect("get after reopen");
    }

    #[tokio::test]
    async fn count_returns_zero_for_empty_log() {
        let log = EventLog::open_in_memory().await.unwrap();
        assert_eq!(log.count().await.unwrap(), 0);
    }

    #[tokio::test]
    async fn get_returns_not_found_for_missing_version() {
        let log = EventLog::open_in_memory().await.unwrap();
        let err = log.get(&VersionId::new()).await.unwrap_err();
        assert!(matches!(err, LogError::NotFound(_)));
    }
}
