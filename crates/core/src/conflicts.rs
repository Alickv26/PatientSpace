//! Conflict detection and resolution endpoints.
//!
//! Scans the event log for resources with concurrent versions and returns
//! them to the UI for clinician review.

use std::collections::HashMap;

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::Json;
use serde::Serialize;
use tracing::info;
use uuid::Uuid;

use crate::http::AppState;

#[derive(Debug, Serialize)]
pub struct ConflictSummary {
    pub resource_id: String,
    pub resource_type: String,
    pub conflicting_versions: Vec<ConflictVersion>,
}

#[derive(Debug, Serialize)]
pub struct ConflictVersion {
    pub version_id: String,
    pub author_facility_id: String,
    pub created_at_unix_ms: u64,
    pub resource: serde_json::Value,
}

/// GET /api/conflicts — list all resources with concurrent versions.
pub async fn list_conflicts(State(state): State<AppState>) -> impl IntoResponse {
    // Scan all resource types for conflicts.
    let resource_types = ["Patient", "Encounter", "Observation", "MedicationRequest", "Condition"];

    let mut conflicts: Vec<ConflictSummary> = Vec::new();

    for rtype in &resource_types {
        let envelopes = match state.log.list_by_type(rtype, 10_000).await {
            Ok(v) => v,
            Err(_) => continue,
        };

        // Group by resource_id.
        let mut by_resource: HashMap<Uuid, Vec<&patientspace_sync::Envelope>> = HashMap::new();
        for env in &envelopes {
            by_resource.entry(env.resource_id).or_default().push(env);
        }

        // Check each resource for conflicts.
        for (resource_id, versions) in &by_resource {
            let envs: Vec<patientspace_sync::Envelope> = versions.iter().map(|e| (*e).clone()).collect();
            let detected = patientspace_sync::detect_conflicts(&envs);

            if detected.is_empty() {
                continue;
            }

            // Decrypt all versions for this resource.
            let mut conflict_versions = Vec::new();
            for env in &envs {
                if let Ok(plaintext) = env.open(&state.facility_id, state.keypair.as_ref()) {
                    if let Ok(resource) = serde_json::from_slice::<serde_json::Value>(&plaintext) {
                        conflict_versions.push(ConflictVersion {
                            version_id: env.version_id.to_string(),
                            author_facility_id: env.author_facility_id.clone(),
                            created_at_unix_ms: env.created_at_unix_ms,
                            resource,
                        });
                    }
                }
            }

            if !conflict_versions.is_empty() {
                conflicts.push(ConflictSummary {
                    resource_id: resource_id.to_string(),
                    resource_type: rtype.to_string(),
                    conflicting_versions: conflict_versions,
                });
            }
        }
    }

    info!(count = conflicts.len(), "found conflicts");

    Json(serde_json::json!({
        "total": conflicts.len(),
        "conflicts": conflicts,
    }))
}

/// GET /api/conflicts/{resource_id} — get conflict details for a specific resource.
pub async fn get_conflict(
    State(state): State<AppState>,
    Path(resource_id_str): Path<String>,
) -> impl IntoResponse {
    let resource_id = match Uuid::parse_str(&resource_id_str) {
        Ok(u) => patientspace_shared::ResourceId(u),
        Err(_) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({"error": "invalid resource_id"})),
            );
        }
    };

    let envelopes = match state.log.list_by_resource(&resource_id).await {
        Ok(v) => v,
        Err(_) => {
            return (
                StatusCode::NOT_FOUND,
                Json(serde_json::json!({"error": "resource not found"})),
            );
        }
    };

    // Check for conflicts.
    let detected = patientspace_sync::detect_conflicts(&envelopes);
    if detected.is_empty() {
        return (
            StatusCode::OK,
            Json(serde_json::json!({
                "resource_id": resource_id_str,
                "has_conflicts": false,
                "versions": [],
            })),
        );
    }

    // Decrypt all versions.
    let mut versions = Vec::new();
    for env in &envelopes {
        if let Ok(plaintext) = env.open(&state.facility_id, state.keypair.as_ref()) {
            if let Ok(resource) = serde_json::from_slice::<serde_json::Value>(&plaintext) {
                versions.push(ConflictVersion {
                    version_id: env.version_id.to_string(),
                    author_facility_id: env.author_facility_id.clone(),
                    created_at_unix_ms: env.created_at_unix_ms,
                    resource,
                });
            }
        }
    }

    (
        StatusCode::OK,
        Json(serde_json::json!({
            "resource_id": resource_id_str,
            "has_conflicts": true,
            "conflict_pairs": detected.iter().map(|c| {
                serde_json::json!({
                    "version_a": c.version_a.to_string(),
                    "version_b": c.version_b.to_string(),
                    "resource_id": c.resource_id.to_string(),
                })
            }).collect::<Vec<_>>(),
            "versions": versions,
        })),
    )
}
