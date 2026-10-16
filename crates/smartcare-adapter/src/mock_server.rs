//! Mock SmartCare HIE server for testing.
//!
//! Serves synthetic FHIR R4 Patient resources at `http://127.0.0.1:9090/fhir`.
//! Used by the migration adapter tests and the `patientspace migrate` CLI
//! for dry-run validation.

use std::net::SocketAddr;

use axum::{routing::get, Json, Router};
use serde::Serialize;
use tracing::info;

/// Configuration for the mock server.
#[derive(Debug, Clone)]
pub struct MockServerConfig {
    pub bind: String,
    pub num_patients: u32,
}

impl Default for MockServerConfig {
    fn default() -> Self {
        Self {
            bind: "127.0.0.1:9090".to_string(),
            num_patients: 100,
        }
    }
}

/// Start the mock SmartCare HIE server.
pub async fn start_server(config: MockServerConfig) -> anyhow::Result<()> {
    let patients = generate_mock_patients(config.num_patients);

    let app = Router::new()
        .route(
            "/fhir/Patient",
            get(move || {
                let patients = patients.clone();
                async move { Json(build_bundle(&patients)) }
            }),
        )
        .route("/fhir/metadata", get(|| async { Json(serde_json::json!({"resourceType": "CapabilityStatement"})) }));

    let addr: SocketAddr = config.bind.parse()?;
    let listener = tokio::net::TcpListener::bind(addr).await?;
    info!(%addr, patients = config.num_patients, "mock SmartCare HIE listening");
    axum::serve(listener, app).await?;
    Ok(())
}

#[derive(Serialize)]
struct FhirBundle {
    resource_type: String,
    #[serde(rename = "type")]
    bundle_type: String,
    total: u32,
    entry: Vec<FhirEntry>,
}

#[derive(Serialize)]
struct FhirEntry {
    resource: serde_json::Value,
}

fn build_bundle(patients: &[serde_json::Value]) -> FhirBundle {
    FhirBundle {
        resource_type: "Bundle".to_string(),
        bundle_type: "searchset".to_string(),
        total: patients.len() as u32,
        entry: patients
            .iter()
            .map(|p| FhirEntry { resource: p.clone() })
            .collect(),
    }
}

fn generate_mock_patients(count: u32) -> Vec<serde_json::Value> {
    let surnames = [
        "Banda", "Phiri", "Tembo", "Mwale", "Zulu", "Nyirenda", "Chirwa",
        "Lungu", "Mumba", "Sakala", "Chisanga", "Kabwe", "Musonda",
    ];
    let given_names = [
        "Chipo", "John", "Mary", "Emmanuel", "Grace", "David", "Ruth",
        "Joseph", "Esther", "Daniel", "Sarah", "Peter", "Anna", "Moses",
    ];
    let districts = ["Petauke", "Nyimba", "Mambwe", "Katete", "Sinda"];

    let mut patients = Vec::with_capacity(count as usize);
    for i in 0..count {
        let surname = surnames[(i as usize) % surnames.len()];
        let given = given_names[(i as usize) % given_names.len()];
        let district = districts[(i as usize) % districts.len()];
        let gender = if i % 2 == 0 { "female" } else { "male" };
        let year = 1940 + (i % 80);
        let month = 1 + (i % 12);
        let day = 1 + (i % 28);
        let smartcard = format!("SC-{:08}", 10000000 + i);
        let nrc = format!("{:06}/{:02}/{}", 100000 + i, 10 + (i % 80), (i % 9) + 1);

        patients.push(serde_json::json!({
            "resourceType": "Patient",
            "id": format!("smartcare-{}", i),
            "identifier": [
                {"system": "http://smartcare.org/sid/smartcard", "value": smartcard},
                {"system": "http://smartcare.org/sid/nrc-zm", "value": nrc}
            ],
            "name": [{
                "text": format!("{}, {}", surname, given),
                "given": [given],
                "family": surname,
                "use": "official"
            }],
            "telecom": [{
                "system": "phone",
                "value": format!("+260 97 {:07}", 1000000 + i),
                "use": "mobile"
            }],
            "gender": gender,
            "birthDate": format!("{:04}-{:02}-{:02}", year, month, day),
            "address": [{
                "district": district,
                "city": district,
                "country": "Zambia"
            }]
        }));
    }
    patients
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generate_mock_patients_produces_valid_count() {
        let patients = generate_mock_patients(50);
        assert_eq!(patients.len(), 50);
        assert_eq!(patients[0]["resourceType"], "Patient");
        assert!(patients[0]["identifier"].is_array());
    }
}
