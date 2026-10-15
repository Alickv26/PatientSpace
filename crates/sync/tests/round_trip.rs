//! Integration test: byte-identical round-trip of 1000 random envelopes
//! through the SQLite-backed event log.
//!
//! Generates a realistic mix of envelopes across 5 facilities, 4 resource
//! types, and 20 resources, with mixed parent patterns, varying payload
//! sizes (200B–4KB), random vector clocks, and random timestamps. Then
//! verifies that every envelope survives the SQLite write → read cycle
//! byte-for-byte.

use patientspace_shared::{
    ClinicianId, FacilityId, ResourceId, ResourceKind, VersionId,
};
use patientspace_sync::{
    Envelope, EnvelopeBuilder, EventLog, FacilityKeypair, FacilityPublicKeys, VectorClock,
};
use rand::distr::{Alphanumeric, SampleString};
use rand::RngExt;
use uuid::Uuid;

#[tokio::test]
async fn round_trip_1000_envelopes_through_sqlite() {
    let log = EventLog::open_in_memory().await.expect("open log");

    // 5 facilities, each with its own keypair.
    let facilities: Vec<(FacilityId, FacilityKeypair)> = (0..5)
        .map(|i| (FacilityId::new(format!("F-FAC{i}")), FacilityKeypair::generate()))
        .collect();

    // 20 resources spread across 4 resource types.
    let resource_types = ["Patient", "Encounter", "Observation", "MedicationRequest"];
    let resources: Vec<(ResourceId, &str)> = (0..20)
        .map(|i| (ResourceId::new(), resource_types[i % 4]))
        .collect();

    let mut rng = rand::rng();

    // Track original CBOR bytes and version IDs for verification.
    let mut original_cbor: Vec<Vec<u8>> = Vec::with_capacity(1000);
    let mut original_version_ids: Vec<Uuid> = Vec::with_capacity(1000);

    for _ in 0..1000 {
        // Pick a random facility and resource.
        let (facility_id, keypair) = &facilities[rng.random_range(0..facilities.len())];
        let (resource_id, resource_type) = &resources[rng.random_range(0..resources.len())];

        // Random vector clock: observe some facilities, then increment the author.
        let mut vc = VectorClock::new();
        for (fid, _) in &facilities {
            if rand::random_bool(0.5) {
                vc.observe(fid, rng.random_range(1..100));
            }
        }
        vc.increment(facility_id);

        // Random parents (0, 1, or 2 from previously generated envelopes).
        let parent_count = rng.random_range(0..=2usize);
        let parents: Vec<VersionId> = (0..parent_count)
            .filter_map(|_| {
                if original_version_ids.is_empty() {
                    None
                } else {
                    let idx = rng.random_range(0..original_version_ids.len());
                    Some(VersionId(original_version_ids[idx]))
                }
            })
            .collect();

        // Random payload: 200–4096 alphanumeric characters as FHIR narrative text.
        let payload_size = rng.random_range(200..=4096);
        let payload_string = Alphanumeric.sample_string(&mut rng, payload_size);

        let payload = serde_json::json!({
            "resourceType": resource_type,
            "id": resource_id.to_string(),
            "text": {
                "status": "generated",
                "div": format!("<div>{payload_string}</div>"),
            }
        });
        let plaintext = serde_json::to_vec(&payload).expect("serialize payload");

        // Random timestamp (2023–2027 range).
        let created_at = rng.random_range(1_700_000_000_000u64..1_800_000_000_000);

        let author_pub = keypair.public_keys();
        let builder = EnvelopeBuilder {
            version_id: VersionId::new(),
            resource_id: resource_id.clone(),
            resource_type: resource_type.to_string(),
            vector_clock: vc,
            parent_version_ids: parents,
            author_facility_id: facility_id.clone(),
            author_clinician_id: ClinicianId::new(),
            created_at_unix_ms: created_at,
            kind: ResourceKind::Clinical,
            plaintext,
        };

        // Seal the envelope (author is its own recipient).
        let recipients: Vec<(&FacilityId, &FacilityPublicKeys)> = vec![(facility_id, &author_pub)];
        let env: Envelope = builder.seal(keypair, &recipients).expect("seal envelope");

        // Record the original CBOR and version_id for later comparison.
        let cbor = env.to_cbor().expect("encode envelope");
        original_version_ids.push(env.version_id);
        original_cbor.push(cbor);

        // Write to the log.
        log.ingest(&env).await.expect("ingest");
    }

    // Sanity: all 1000 envelopes made it in.
    assert_eq!(log.count().await.expect("count"), 1000);

    // Read each one back and verify byte-identical CBOR.
    for (i, version_id) in original_version_ids.iter().enumerate() {
        let retrieved = log.get(&VersionId(*version_id)).await.expect("get");
        let retrieved_cbor = retrieved.to_cbor().expect("re-encode");
        assert_eq!(
            original_cbor[i],
            retrieved_cbor,
            "byte mismatch at envelope #{i} (version_id={version_id})"
        );
    }
}
