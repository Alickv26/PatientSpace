//! Integration tests for the FHIR Patient REST facade.
//!
//! These boot a real axum server on a random port backed by an in-memory
//! PSEP event log, then drive it via `reqwest`. They cover the happy paths
//! and the error paths for the four endpoints shipped in v0.1:
//!   - `GET  /health`
//!   - `POST /fhir/Patient`
//!   - `GET  /fhir/Patient`
//!   - `GET  /fhir/Patient/{id}`

use std::sync::Arc;

use axum::{routing::get, Router};
use serde_json::{json, Value};

use patientspace_core::http::{create_patient, get_patient, health, list_patients, AppState};
use patientspace_shared::FacilityId;
use patientspace_sync::{EventLog, FacilityKeypair};

/// Start a fresh core instance on a random port.
///
/// Returns the base URL (e.g. `http://127.0.0.1:54321`) and a clone of the
/// in-memory event log the server is using. The server runs in a background
/// tokio task for the lifetime of the test (the runtime is dropped when the
/// test function returns, which cancels the task).
async fn start_core() -> (String, EventLog) {
    let log = EventLog::open_in_memory().await.expect("open log");
    let keypair = Arc::new(FacilityKeypair::generate());
    let facility_id = FacilityId::new("F-TEST");
    let state = AppState {
        facility_id,
        keypair,
        log: log.clone(),
    };

    let app = Router::new()
        .route("/health", get(health))
        .route("/fhir/Patient", get(list_patients).post(create_patient))
        .route("/fhir/Patient/{id}", get(get_patient))
        .with_state(state);

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let url = format!("http://{addr}");

    tokio::spawn(async move {
        // Errors here are fine — the listener is dropped when the test exits.
        let _ = axum::serve(listener, app).await;
    });

    (url, log)
}

/// Build a minimal Patient JSON payload for POSTing.
fn patient_payload(smartcard: &str, name: &str, gender: &str, birth: &str) -> Value {
    json!({
        "resourceType": "Patient",
        "identifier": [{
            "system": "http://patientspace.org/sid/smartcard",
            "value": smartcard,
        }],
        "name": [{ "text": name }],
        "gender": gender,
        "birthDate": birth,
    })
}

#[tokio::test]
async fn create_and_get_patient() {
    let (url, _log) = start_core().await;
    let client = reqwest::Client::new();

    // POST → 201 Created
    let body = patient_payload("SC-001", "Banda, Chipo", "female", "1990-04-12");
    let resp = client
        .post(format!("{url}/fhir/Patient"))
        .json(&body)
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 201);
    let created: Value = resp.json().await.unwrap();

    // Verify all fields echoed back, with a fresh U-<uuid> id assigned.
    let id = created["id"].as_str().expect("id").to_string();
    assert!(
        id.starts_with("U-"),
        "id should be U-<uuid>, got {id}"
    );
    assert_eq!(created["resourceType"], "Patient");
    assert_eq!(created["identifier"][0]["value"], "SC-001");
    assert_eq!(created["name"][0]["text"], "Banda, Chipo");
    assert_eq!(created["gender"], "female");
    assert_eq!(created["birthDate"], "1990-04-12");

    // GET the same id → 200 OK with matching fields
    let resp = client
        .get(format!("{url}/fhir/Patient/{id}"))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let fetched: Value = resp.json().await.unwrap();
    assert_eq!(fetched["id"], id);
    assert_eq!(fetched["resourceType"], "Patient");
    assert_eq!(fetched["identifier"][0]["value"], "SC-001");
    assert_eq!(fetched["name"][0]["text"], "Banda, Chipo");
    assert_eq!(fetched["gender"], "female");
    assert_eq!(fetched["birthDate"], "1990-04-12");
}

#[tokio::test]
async fn get_patient_returns_404_for_unknown() {
    let (url, _log) = start_core().await;
    let client = reqwest::Client::new();

    let random = uuid::Uuid::now_v7();
    let resp = client
        .get(format!("{url}/fhir/Patient/U-{random}"))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 404);
    let body: Value = resp.json().await.unwrap();
    assert_eq!(body["resourceType"], "OperationOutcome");
    assert_eq!(body["issue"][0]["severity"], "not-found");
}

#[tokio::test]
async fn list_patients_returns_bundle() {
    let (url, _log) = start_core().await;
    let client = reqwest::Client::new();

    for i in 1..=3 {
        let body = patient_payload(
            &format!("SC-{i:03}"),
            &format!("Patient {i}"),
            "female",
            "1990-01-01",
        );
        let resp = client
            .post(format!("{url}/fhir/Patient"))
            .json(&body)
            .send()
            .await
            .unwrap();
        assert_eq!(resp.status(), 201);
    }

    let resp = client
        .get(format!("{url}/fhir/Patient"))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let bundle: Value = resp.json().await.unwrap();
    assert_eq!(bundle["resourceType"], "Bundle");
    assert_eq!(bundle["type"], "searchset");
    assert_eq!(bundle["total"], 3);
    assert_eq!(bundle["entry"].as_array().unwrap().len(), 3);
}

#[tokio::test]
async fn list_patients_with_pagination() {
    let (url, _log) = start_core().await;
    let client = reqwest::Client::new();

    for i in 1..=5 {
        let body = patient_payload(
            &format!("SC-{i:03}"),
            &format!("Patient {i}"),
            "female",
            "1990-01-01",
        );
        let _ = client
            .post(format!("{url}/fhir/Patient"))
            .json(&body)
            .send()
            .await
            .unwrap();
    }

    // _count=2, _offset=0 → 2 entries, total=5
    let resp = client
        .get(format!("{url}/fhir/Patient?_count=2&_offset=0"))
        .send()
        .await
        .unwrap();
    let b: Value = resp.json().await.unwrap();
    assert_eq!(b["total"], 5);
    assert_eq!(b["entry"].as_array().unwrap().len(), 2);

    // _count=2, _offset=2 → 2 entries
    let resp = client
        .get(format!("{url}/fhir/Patient?_count=2&_offset=2"))
        .send()
        .await
        .unwrap();
    let b: Value = resp.json().await.unwrap();
    assert_eq!(b["total"], 5);
    assert_eq!(b["entry"].as_array().unwrap().len(), 2);

    // _count=2, _offset=4 → 1 entry (last page)
    let resp = client
        .get(format!("{url}/fhir/Patient?_count=2&_offset=4"))
        .send()
        .await
        .unwrap();
    let b: Value = resp.json().await.unwrap();
    assert_eq!(b["total"], 5);
    assert_eq!(b["entry"].as_array().unwrap().len(), 1);
}

#[tokio::test]
async fn search_by_smartcard() {
    let (url, _log) = start_core().await;
    let client = reqwest::Client::new();

    for sc in ["SC-001", "SC-002", "SC-003"] {
        let body = patient_payload(sc, &format!("Patient {sc}"), "female", "1990-01-01");
        let _ = client
            .post(format!("{url}/fhir/Patient"))
            .json(&body)
            .send()
            .await
            .unwrap();
    }

    // Search by value-only identifier filter → exactly 1 match.
    let resp = client
        .get(format!("{url}/fhir/Patient"))
        .query(&[("identifier", "SC-002")])
        .send()
        .await
        .unwrap();
    let b: Value = resp.json().await.unwrap();
    assert_eq!(b["total"], 1);
    let entry = &b["entry"][0]["resource"];
    assert_eq!(entry["identifier"][0]["value"], "SC-002");
}

#[tokio::test]
async fn search_by_nrc() {
    let (url, _log) = start_core().await;
    let client = reqwest::Client::new();

    // Three patients with different NRC values under the nrc-zm system.
    for nrc in ["123456/78/1", "234567/89/2", "345678/90/3"] {
        let body = json!({
            "resourceType": "Patient",
            "identifier": [{
                "system": "http://patientspace.org/sid/nrc-zm",
                "value": nrc,
            }],
            "name": [{ "text": format!("Patient {nrc}") }],
        });
        let _ = client
            .post(format!("{url}/fhir/Patient"))
            .json(&body)
            .send()
            .await
            .unwrap();
    }

    // system|value filter → exactly 1 match.
    let filter = "http://patientspace.org/sid/nrc-zm|234567/89/2";
    let resp = client
        .get(format!("{url}/fhir/Patient"))
        .query(&[("identifier", filter)])
        .send()
        .await
        .unwrap();
    let b: Value = resp.json().await.unwrap();
    assert_eq!(b["total"], 1);
    let entry = &b["entry"][0]["resource"];
    assert_eq!(
        entry["identifier"][0]["system"],
        "http://patientspace.org/sid/nrc-zm"
    );
    assert_eq!(entry["identifier"][0]["value"], "234567/89/2");
}

#[tokio::test]
async fn health_returns_envelope_count() {
    let (url, _log) = start_core().await;
    let client = reqwest::Client::new();

    // Before any ingestion.
    let resp = client.get(format!("{url}/health")).send().await.unwrap();
    let h: Value = resp.json().await.unwrap();
    assert_eq!(h["status"], "ok");
    assert_eq!(h["facility_id"], "F-TEST");
    assert_eq!(h["envelope_count"], 0);

    // Create 2 patients.
    for i in 1..=2 {
        let body = patient_payload(
            &format!("SC-{i:03}"),
            &format!("P{i}"),
            "female",
            "1990-01-01",
        );
        let _ = client
            .post(format!("{url}/fhir/Patient"))
            .json(&body)
            .send()
            .await
            .unwrap();
    }

    let resp = client.get(format!("{url}/health")).send().await.unwrap();
    let h: Value = resp.json().await.unwrap();
    assert_eq!(h["envelope_count"], 2);
}

#[tokio::test]
async fn post_invalid_json_returns_400() {
    let (url, _log) = start_core().await;
    let client = reqwest::Client::new();

    let resp = client
        .post(format!("{url}/fhir/Patient"))
        .header("content-type", "application/json")
        .body("not valid json")
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 400);
    let body: Value = resp.json().await.unwrap();
    assert_eq!(body["resourceType"], "OperationOutcome");
    assert_eq!(body["issue"][0]["severity"], "invalid");
}
