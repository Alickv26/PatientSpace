//! Axum handlers for the FHIR REST facade.
//!
//! Endpoints:
//!   - `GET  /health`                        — liveness probe + envelope count
//!   - `POST /fhir/Patient`                  — create a Patient (seals + ingests a PSEP envelope)
//!   - `GET  /fhir/Patient`                  — list Patients (paginated, with optional identifier search)
//!   - `GET  /fhir/Patient/{id}`             — fetch a single Patient by resource_id
//!   - `POST /fhir/{type}`                   — create any FHIR resource (Encounter, Observation, etc.)
//!   - `GET  /fhir/{type}`                   — list any FHIR resource type
//!   - `GET  /fhir/{type}/{id}`             — fetch any FHIR resource by resource_id
//!
//! All write paths go through [`patientspace_sync::EnvelopeBuilder`] — the
//! local SQLite log is the source of truth, never bypassed. Reads decrypt
//! the latest version of each resource with the facility's keypair.

use std::collections::HashMap;
use std::sync::Arc;

use axum::body::Bytes;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Json};
use patientspace_shared::{ClinicianId, FacilityId, ResourceId, ResourceKind, VersionId};
use patientspace_sync::{
    detect_conflicts, Envelope, EnvelopeBuilder, EventLog, FacilityKeypair, VectorClock,
};
use serde::Deserialize;
use serde_json::{json, Value};
use uuid::Uuid;

/// Shared state handed to every axum handler.
///
/// Cheaply clonable — `EventLog` wraps a `SqlitePool` internally, and the
/// `FacilityKeypair` is hidden behind an `Arc` so we never clone the
/// secret signing/encryption material.
#[derive(Clone)]
pub struct AppState {
    pub facility_id: FacilityId,
    pub keypair: Arc<FacilityKeypair>,
    pub log: EventLog,
}

/// Query parameters accepted by `GET /fhir/Patient`.
#[derive(Debug, Deserialize, Default)]
pub struct PatientQuery {
    /// Page size. Defaults to 50, clamped to `[1, 200]`.
    #[serde(rename = "_count")]
    pub count: Option<u32>,
    /// Page offset (zero-based). Defaults to 0.
    #[serde(rename = "_offset")]
    pub offset: Option<u32>,
    /// Optional identifier filter. Accepts either `value` (matches any
    /// identifier regardless of system) or `system|value` (matches the
    /// given system URL exactly).
    pub identifier: Option<String>,
}

// -----------------------------------------------------------------
// Handlers
// -----------------------------------------------------------------

/// `GET /health` — liveness + diagnostics.
pub async fn health(State(state): State<AppState>) -> impl IntoResponse {
    let envelope_count = state.log.count().await.unwrap_or(0);
    Json(json!({
        "status": "ok",
        "facility_id": state.facility_id.as_str(),
        "version": env!("CARGO_PKG_VERSION"),
        "envelope_count": envelope_count,
    }))
}

/// `POST /fhir/Patient` — create a new Patient.
///
/// Parses the request body as a FHIR Patient JSON object, assigns a fresh
/// `resource_id` (overwriting any client-supplied `id`), seals it into a
/// PSEP envelope addressed to the local facility, and ingests the envelope
/// into the event log. Returns `201 Created` with the canonical Patient JSON.
pub async fn create_patient(
    State(state): State<AppState>,
    body: Bytes,
) -> impl IntoResponse {
    // 1. Parse body as JSON.
    let mut patient: Value = match serde_json::from_slice(&body) {
        Ok(v) => v,
        Err(e) => return outcome(StatusCode::BAD_REQUEST, "invalid", &e.to_string()),
    };

    // 2. Validate / set resourceType.
    match patient.get("resourceType").and_then(Value::as_str) {
        Some("Patient") => {}
        Some(other) => {
            return outcome(
                StatusCode::BAD_REQUEST,
                "invalid",
                &format!("expected resourceType=Patient, got {other}"),
            );
        }
        None => {
            patient["resourceType"] = json!("Patient");
        }
    }

    // 3. Assign a fresh resource_id. The server is authoritative for ids.
    let resource_id = ResourceId::new();
    patient["id"] = json!(resource_id.to_string());

    // 4. Serialize canonical bytes for the envelope payload.
    let plaintext = match serde_json::to_vec(&patient) {
        Ok(b) => b,
        Err(e) => return outcome(StatusCode::INTERNAL_SERVER_ERROR, "exception", &e.to_string()),
    };

    // 5. Build the envelope. Genesis version: no parents, vector clock at
    //    counter 1 for the authoring facility.
    let author_pub = state.keypair.public_keys();
    let builder = EnvelopeBuilder {
        version_id: VersionId::new(),
        resource_id: resource_id.clone(),
        resource_type: "Patient".to_string(),
        vector_clock: VectorClock::from_facility(&state.facility_id, 1),
        parent_version_ids: vec![],
        author_facility_id: state.facility_id.clone(),
        author_clinician_id: ClinicianId::new(),
        created_at_unix_ms: now_unix_ms(),
        kind: ResourceKind::Clinical,
        plaintext,
    };

    // 6. Seal — the facility is the sole recipient of its own events.
    let recipients = vec![(&state.facility_id, &author_pub)];
    let envelope = match builder.seal(state.keypair.as_ref(), &recipients) {
        Ok(e) => e,
        Err(e) => return outcome(StatusCode::INTERNAL_SERVER_ERROR, "exception", &e.to_string()),
    };

    // 7. Ingest into the append-only log.
    if let Err(e) = state.log.ingest(&envelope).await {
        return outcome(StatusCode::INTERNAL_SERVER_ERROR, "exception", &e.to_string());
    }

    // 8. 201 Created with the canonical Patient JSON.
    (StatusCode::CREATED, Json(patient))
}

/// `GET /fhir/Patient/{id}` — fetch a single Patient by resource_id.
///
/// Resolves to the latest version of the resource known to the local log.
/// If the resource has concurrent versions (a conflict), the response is
/// annotated with a PatientSpace conflict extension.
pub async fn get_patient(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    let resource_id = match parse_resource_id(&id) {
        Some(r) => r,
        None => {
            return outcome(
                StatusCode::NOT_FOUND,
                "not-found",
                &format!("Patient {id} not found"),
            );
        }
    };

    // Latest version of the resource.
    let latest = match state.log.latest_for_resource(&resource_id).await {
        Ok(env) => env,
        Err(patientspace_sync::LogError::NotFound(_)) => {
            return outcome(
                StatusCode::NOT_FOUND,
                "not-found",
                &format!("Patient {id} not found"),
            );
        }
        Err(e) => return outcome(StatusCode::INTERNAL_SERVER_ERROR, "exception", &e.to_string()),
    };

    // Decrypt with the facility keypair.
    let plaintext = match latest.open(&state.facility_id, state.keypair.as_ref()) {
        Ok(b) => b,
        Err(e) => return outcome(StatusCode::INTERNAL_SERVER_ERROR, "exception", &e.to_string()),
    };

    let mut patient: Value = match serde_json::from_slice(&plaintext) {
        Ok(v) => v,
        Err(e) => return outcome(StatusCode::INTERNAL_SERVER_ERROR, "exception", &e.to_string()),
    };

    // Conflict check: scan all known versions of this resource for any
    // concurrent pair. If found, annotate the response with a PatientSpace
    // conflict extension so the clinician UI can surface it.
    let all_versions = state
        .log
        .list_by_resource(&resource_id)
        .await
        .unwrap_or_default();
    let conflicts = detect_conflicts(&all_versions);
    if !conflicts.is_empty() {
        let ext = json!({
            "url": "http://patientspace.org/fhir/StructureDefinition/conflict",
            "valueString": format!("{} unresolved conflict(s)", conflicts.len()),
        });
        match patient.get_mut("extension").and_then(Value::as_array_mut) {
            Some(arr) => arr.push(ext),
            None => {
                patient["extension"] = json!([ext]);
            }
        }
    }

    (StatusCode::OK, Json(patient))
}

/// `GET /fhir/Patient` — list/search Patients.
///
/// Pagination via `_count` / `_offset`. Optional `identifier` filter supports
/// both `value` and `system|value` formats. Returns a FHIR searchset Bundle.
pub async fn list_patients(
    State(state): State<AppState>,
    Query(q): Query<PatientQuery>,
) -> impl IntoResponse {
    let count = q.count.unwrap_or(50).clamp(1, 200);
    let offset = q.offset.unwrap_or(0);

    // Pull a generous batch of recent Patient envelopes, decrypt each, and
    // dedup to the latest version per resource_id.
    let envelopes = match state.log.list_by_type("Patient", 10_000).await {
        Ok(v) => v,
        Err(e) => return outcome(StatusCode::INTERNAL_SERVER_ERROR, "exception", &e.to_string()),
    };

    // list_by_type returns newest-first; first sighting per resource_id wins.
    let mut latest_per_resource: HashMap<Uuid, Envelope> = HashMap::new();
    for env in envelopes {
        latest_per_resource.entry(env.resource_id).or_insert(env);
    }
    let mut latest: Vec<Envelope> = latest_per_resource.into_values().collect();
    // Stable order by created_at desc (descending).
    latest.sort_by(|a, b| b.created_at_unix_ms.cmp(&a.created_at_unix_ms));

    // Decrypt + filter.
    let mut matches: Vec<Value> = Vec::new();
    for env in &latest {
        let plaintext = match env.open(&state.facility_id, state.keypair.as_ref()) {
            Ok(b) => b,
            Err(_) => continue,
        };
        let patient: Value = match serde_json::from_slice(&plaintext) {
            Ok(v) => v,
            Err(_) => continue,
        };
        if let Some(filter) = q.identifier.as_deref() {
            if !patient_matches_identifier(&patient, filter) {
                continue;
            }
        }
        matches.push(patient);
    }

    let total = matches.len() as u32;
    let paged: Vec<Value> = matches
        .into_iter()
        .skip(offset as usize)
        .take(count as usize)
        .collect();

    let bundle = json!({
        "resourceType": "Bundle",
        "type": "searchset",
        "total": total,
        "entry": paged.into_iter().map(|p| json!({ "resource": p })).collect::<Vec<_>>(),
    });

    (StatusCode::OK, Json(bundle))
}

// -----------------------------------------------------------------
// Helpers
// -----------------------------------------------------------------

/// Build a FHIR OperationOutcome JSON response tuple.
fn outcome(status: StatusCode, severity: &str, diagnostics: &str) -> (StatusCode, Json<Value>) {
    (
        status,
        Json(json!({
            "resourceType": "OperationOutcome",
            "issue": [{
                "severity": severity,
                "code": "exception",
                "diagnostics": diagnostics,
            }],
        })),
    )
}

/// Parse a Patient id from a path segment.
///
/// Accepts both `U-{uuid}` (the canonical PatientSpace format) and a bare
/// `{uuid}`. Returns `None` if neither parses.
fn parse_resource_id(s: &str) -> Option<ResourceId> {
    let raw = s.strip_prefix("U-").unwrap_or(s);
    Uuid::parse_str(raw).ok().map(ResourceId)
}

/// Check whether a Patient's `identifier` array matches the given filter.
///
/// `filter` is either `"value"` (matches any identifier whose `value` equals
/// the filter) or `"system|value"` (matches an identifier whose `system` and
/// `value` both match).
fn patient_matches_identifier(patient: &Value, filter: &str) -> bool {
    let Some(identifiers) = patient.get("identifier").and_then(Value::as_array) else {
        return false;
    };
    let (filter_system, filter_value) = match filter.split_once('|') {
        Some((sys, val)) => (Some(sys), val),
        None => (None, filter),
    };
    identifiers.iter().any(|id| {
        let val = id.get("value").and_then(Value::as_str).unwrap_or("");
        if val != filter_value {
            return false;
        }
        match filter_system {
            None => true,
            Some(sys) => id
                .get("system")
                .and_then(Value::as_str)
                .map_or(false, |s| s == sys),
        }
    })
}

/// Current unix time in milliseconds. Wraps to 0 if the system clock is
/// before the epoch (which would be a much bigger problem than this function).
fn now_unix_ms() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

// -----------------------------------------------------------------
// Generic FHIR resource endpoints (Encounter, Observation, etc.)
// -----------------------------------------------------------------

/// `POST /fhir/{type}` — create any FHIR resource.
pub async fn create_resource(
    State(state): State<AppState>,
    Path(resource_type): Path<String>,
    body: Bytes,
) -> impl IntoResponse {
    let mut resource: Value = match serde_json::from_slice(&body) {
        Ok(v) => v,
        Err(e) => return outcome(StatusCode::BAD_REQUEST, "invalid", &e.to_string()),
    };

    match resource.get("resourceType").and_then(Value::as_str) {
        Some(rt) if rt == resource_type => {}
        Some(other) => {
            return outcome(StatusCode::BAD_REQUEST, "invalid",
                &format!("expected resourceType={resource_type}, got {other}"));
        }
        None => { resource["resourceType"] = json!(resource_type); }
    }

    let resource_id = ResourceId::new();
    resource["id"] = json!(resource_id.to_string());

    let plaintext = match serde_json::to_vec(&resource) {
        Ok(b) => b,
        Err(e) => return outcome(StatusCode::INTERNAL_SERVER_ERROR, "exception", &e.to_string()),
    };

    let author_pub = state.keypair.public_keys();
    let builder = EnvelopeBuilder {
        version_id: VersionId::new(),
        resource_id: resource_id.clone(),
        resource_type: resource_type.clone(),
        vector_clock: VectorClock::from_facility(&state.facility_id, 1),
        parent_version_ids: vec![],
        author_facility_id: state.facility_id.clone(),
        author_clinician_id: ClinicianId::new(),
        created_at_unix_ms: now_unix_ms(),
        kind: ResourceKind::Clinical,
        plaintext,
    };

    let recipients = vec![(&state.facility_id, &author_pub)];
    let envelope = match builder.seal(state.keypair.as_ref(), &recipients) {
        Ok(e) => e,
        Err(e) => return outcome(StatusCode::INTERNAL_SERVER_ERROR, "exception", &e.to_string()),
    };

    if let Err(e) = state.log.ingest(&envelope).await {
        return outcome(StatusCode::INTERNAL_SERVER_ERROR, "exception", &e.to_string());
    }

    tracing::info!(%resource_type, %resource_id, "FHIR resource created");
    (StatusCode::CREATED, Json(resource))
}

/// `GET /fhir/{type}` — list any FHIR resource type.
pub async fn list_resources(
    State(state): State<AppState>,
    Path(resource_type): Path<String>,
    Query(q): Query<PatientQuery>,
) -> impl IntoResponse {
    let count = q.count.unwrap_or(50).clamp(1, 200);
    let offset = q.offset.unwrap_or(0);

    let envelopes = match state.log.list_by_type(&resource_type, 10_000).await {
        Ok(v) => v,
        Err(e) => return outcome(StatusCode::INTERNAL_SERVER_ERROR, "exception", &e.to_string()),
    };

    let mut latest_per_resource: HashMap<Uuid, Envelope> = HashMap::new();
    for env in envelopes {
        latest_per_resource.entry(env.resource_id).or_insert(env);
    }
    let mut latest: Vec<Envelope> = latest_per_resource.into_values().collect();
    latest.sort_by(|a, b| b.created_at_unix_ms.cmp(&a.created_at_unix_ms));

    let mut matches: Vec<Value> = Vec::new();
    for env in &latest {
        if let Ok(plaintext) = env.open(&state.facility_id, state.keypair.as_ref()) {
            if let Ok(resource) = serde_json::from_slice::<Value>(&plaintext) {
                matches.push(resource);
            }
        }
    }

    let total = matches.len() as u32;
    let paginated: Vec<Value> = matches.into_iter().skip(offset as usize).take(count as usize).collect();

    Json(json!({
        "resourceType": "Bundle",
        "type": "searchset",
        "total": total,
        "entry": paginated.into_iter().map(|r| json!({"resource": r})).collect::<Vec<_>>(),
    }))
}

/// `GET /fhir/{type}/{id}` — fetch any FHIR resource by id.
pub async fn get_resource(
    State(state): State<AppState>,
    Path((resource_type, id)): Path<(String, String)>,
) -> impl IntoResponse {
    let resource_id = match parse_resource_id(&id) {
        Some(r) => r,
        None => return outcome(StatusCode::NOT_FOUND, "not-found",
            &format!("{resource_type} {id} not found")),
    };

    let latest = match state.log.latest_for_resource(&resource_id).await {
        Ok(env) => env,
        Err(patientspace_sync::LogError::NotFound(_)) => {
            return outcome(StatusCode::NOT_FOUND, "not-found",
                &format!("{resource_type} {id} not found"));
        }
        Err(e) => return outcome(StatusCode::INTERNAL_SERVER_ERROR, "exception", &e.to_string()),
    };

    let plaintext = match latest.open(&state.facility_id, state.keypair.as_ref()) {
        Ok(b) => b,
        Err(e) => return outcome(StatusCode::INTERNAL_SERVER_ERROR, "exception", &e.to_string()),
    };

    let resource: Value = match serde_json::from_slice(&plaintext) {
        Ok(v) => v,
        Err(e) => return outcome(StatusCode::INTERNAL_SERVER_ERROR, "exception", &e.to_string()),
    };

    (StatusCode::OK, Json(resource))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_resource_id_handles_both_forms() {
        let uuid = Uuid::now_v7();
        let a = parse_resource_id(&format!("U-{uuid}")).unwrap();
        let b = parse_resource_id(&format!("{uuid}")).unwrap();
        assert_eq!(a.0, uuid);
        assert_eq!(b.0, uuid);
        assert!(parse_resource_id("not-a-uuid").is_none());
    }

    #[test]
    fn matches_identifier_value_only() {
        let patient = json!({
            "identifier": [
                { "system": "http://patientspace.org/sid/smartcard", "value": "SC-001" },
                { "system": "http://patientspace.org/sid/nrc-zm", "value": "123456/78/1" },
            ],
        });
        assert!(patient_matches_identifier(&patient, "SC-001"));
        assert!(!patient_matches_identifier(&patient, "SC-002"));
        assert!(!patient_matches_identifier(&patient, "system|value"));
    }

    #[test]
    fn matches_identifier_system_and_value() {
        let patient = json!({
            "identifier": [
                { "system": "http://patientspace.org/sid/smartcard", "value": "SC-001" },
                { "system": "http://patientspace.org/sid/nrc-zm", "value": "123456/78/1" },
            ],
        });
        assert!(patient_matches_identifier(
            &patient,
            "http://patientspace.org/sid/smartcard|SC-001",
        ));
        assert!(patient_matches_identifier(
            &patient,
            "http://patientspace.org/sid/nrc-zm|123456/78/1",
        ));
        // Wrong system, right value → no match.
        assert!(!patient_matches_identifier(
            &patient,
            "http://wrong.system|SC-001",
        ));
        // Right system, wrong value → no match.
        assert!(!patient_matches_identifier(
            &patient,
            "http://patientspace.org/sid/smartcard|SC-999",
        ));
    }
}
