use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Json};
use serde::Serialize;
use uuid::Uuid;

#[derive(Clone)]
pub struct AppState {
    pub facility_id: String,
}

pub async fn health() -> impl IntoResponse {
    #[derive(Serialize)]
    struct Health {
        status: &'static str,
        facility_id: String,
        version: &'static str,
    }
    Json(Health {
        status: "ok",
        facility_id: std::env::var("PATIENTSPACE_FACILITY_ID")
            .unwrap_or_else(|_| "F-UNKNOWN".to_string()),
        version: env!("CARGO_PKG_VERSION"),
    })
}

pub async fn list_patients() -> impl IntoResponse {
    // v0.1 stub: return an empty bundle.
    #[derive(Serialize)]
    struct Bundle {
        resource_type: String,
        total: u32,
        entry: Vec<serde_json::Value>,
    }
    Json(Bundle {
        resource_type: "Bundle".to_string(),
        total: 0,
        entry: vec![],
    })
}

pub async fn get_patient(Path(id): Path<String>) -> impl IntoResponse {
    // v0.1 stub: 404 for everything.
    (
        StatusCode::NOT_FOUND,
        format!("Patient {} not found (v0.1 stub)", id),
    )
}

pub async fn create_patient() -> impl IntoResponse {
    // v0.1 stub.
    (
        StatusCode::NOT_IMPLEMENTED,
        "Patient creation not yet implemented (v0.1 stub)",
    )
}
