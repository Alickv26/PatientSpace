//! Point-of-care analytics — computed directly from the event log.
//!
//! Instead of embedding DuckDB (which has long build times), we decrypt the
//! FHIR resources from the event log and compute dashboard metrics in Rust.
//! This gives real-time results with zero additional dependencies.

use std::collections::HashMap;

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::Json;
use serde::Serialize;
use tracing::info;

use crate::http::AppState;

#[derive(Debug, Serialize)]
pub struct DashboardDef {
    pub id: String,
    pub title: String,
    pub description: String,
    pub chart_type: String,
}

pub fn list_dashboards() -> Vec<DashboardDef> {
    vec![
        DashboardDef {
            id: "overview".to_string(),
            title: "Overview".to_string(),
            description: "Total patients, encounters, observations, and prescriptions at this facility.".to_string(),
            chart_type: "kpi".to_string(),
        },
        DashboardDef {
            id: "patient_demographics".to_string(),
            title: "Patient Demographics".to_string(),
            description: "Gender breakdown and age distribution of registered patients.".to_string(),
            chart_type: "bar".to_string(),
        },
        DashboardDef {
            id: "encounters_today".to_string(),
            title: "Encounters Today".to_string(),
            description: "Encounters created today, grouped by hour.".to_string(),
            chart_type: "bar".to_string(),
        },
        DashboardDef {
            id: "recent_activity".to_string(),
            title: "Recent Activity".to_string(),
            description: "Last 20 encounters with patient name, reason, and date.".to_string(),
            chart_type: "table".to_string(),
        },
    ]
}

pub async fn get_dashboards() -> impl IntoResponse {
    Json(serde_json::json!({
        "dashboards": list_dashboards()
    }))
}

pub async fn run_dashboard(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    info!(dashboard_id = %id, "running dashboard");

    let result = match id.as_str() {
        "overview" => run_overview(&state).await,
        "patient_demographics" => run_demographics(&state).await,
        "encounters_today" => run_encounters_today(&state).await,
        "recent_activity" => run_recent_activity(&state).await,
        _ => return (StatusCode::NOT_FOUND, Json(serde_json::json!({"error": "unknown dashboard"}))),
    };

    match result {
        Ok(data) => (StatusCode::OK, Json(data)),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"error": e}))),
    }
}

/// Decrypt all resources of a given type from the event log.
async fn get_resources(
    state: &AppState,
    resource_type: &str,
) -> Vec<serde_json::Value> {
    let envelopes = match state.log.list_by_type(resource_type, 10_000).await {
        Ok(v) => v,
        Err(_) => return vec![],
    };

    let mut latest_per_resource: HashMap<uuid::Uuid, patientspace_sync::Envelope> = HashMap::new();
    for env in envelopes {
        latest_per_resource.entry(env.resource_id).or_insert(env);
    }

    let mut resources = Vec::new();
    for env in latest_per_resource.into_values() {
        if let Ok(plaintext) = env.open(&state.facility_id, state.keypair.as_ref()) {
            if let Ok(resource) = serde_json::from_slice::<serde_json::Value>(&plaintext) {
                resources.push(resource);
            }
        }
    }
    resources
}

async fn run_overview(state: &AppState) -> Result<serde_json::Value, String> {
    let patients = get_resources(state, "Patient").await;
    let encounters = get_resources(state, "Encounter").await;
    let observations = get_resources(state, "Observation").await;
    let medications = get_resources(state, "MedicationRequest").await;
    let conditions = get_resources(state, "Condition").await;

    // Count genders
    let mut male = 0u32;
    let mut female = 0u32;
    let mut other = 0u32;
    for p in &patients {
        match p.get("gender").and_then(|g| g.as_str()) {
            Some("male") => male += 1,
            Some("female") => female += 1,
            _ => other += 1,
        }
    }

    Ok(serde_json::json!({
        "dashboard_id": "overview",
        "chart_type": "kpi",
        "kpis": [
            {"label": "Patients", "value": patients.len()},
            {"label": "Encounters", "value": encounters.len()},
            {"label": "Observations", "value": observations.len()},
            {"label": "Prescriptions", "value": medications.len()},
            {"label": "Conditions", "value": conditions.len()},
            {"label": "Male", "value": male},
            {"label": "Female", "value": female},
        ]
    }))
}

async fn run_demographics(state: &AppState) -> Result<serde_json::Value, String> {
    let patients = get_resources(state, "Patient").await;

    let mut gender_counts: HashMap<String, u32> = HashMap::new();
    let mut age_groups: HashMap<String, u32> = HashMap::new();

    for p in &patients {
        let gender = p.get("gender").and_then(|g| g.as_str()).unwrap_or("unknown").to_string();
        *gender_counts.entry(gender).or_insert(0) += 1;

        // Calculate age from birthDate
        if let Some(birth_date) = p.get("birthDate").and_then(|d| d.as_str()) {
            if let (Ok(year), current_year) = (
                birth_date[..4].parse::<u32>(),
                chrono::Utc::now().format("%Y").to_string().parse::<u32>().unwrap_or(2026),
            ) {
                let age = current_year.saturating_sub(year);
                let group = if age < 5 {
                    "0-4"
                } else if age < 15 {
                    "5-14"
                } else if age < 25 {
                    "15-24"
                } else if age < 45 {
                    "25-44"
                } else if age < 65 {
                    "45-64"
                } else {
                    "65+"
                };
                *age_groups.entry(group.to_string()).or_insert(0) += 1;
            }
        }
    }

    let mut gender_data: Vec<serde_json::Value> = gender_counts
        .into_iter()
        .map(|(k, v)| serde_json::json!({"label": k, "value": v}))
        .collect();
    gender_data.sort_by(|a, b| a["label"].as_str().cmp(&b["label"].as_str()));

    let age_order = ["0-4", "5-14", "15-24", "25-44", "45-64", "65+"];
    let mut age_data: Vec<serde_json::Value> = Vec::new();
    for group in &age_order {
        if let Some(count) = age_groups.get(*group) {
            age_data.push(serde_json::json!({"label": group, "value": count}));
        }
    }

    Ok(serde_json::json!({
        "dashboard_id": "patient_demographics",
        "chart_type": "bar",
        "datasets": [
            {"title": "By Gender", "data": gender_data},
            {"title": "By Age Group", "data": age_data},
        ]
    }))
}

async fn run_encounters_today(state: &AppState) -> Result<serde_json::Value, String> {
    let encounters = get_resources(state, "Encounter").await;
    let today = chrono::Utc::now().format("%Y-%m-%d").to_string();

    let mut hour_counts: HashMap<String, u32> = HashMap::new();
    let mut total_today = 0u32;

    for e in &encounters {
        if let Some(start) = e.get("period").and_then(|p| p.get("start")).and_then(|s| s.as_str()) {
            if start.starts_with(&today) {
                total_today += 1;
                // Extract hour from ISO datetime
                if start.len() >= 13 {
                    let hour = &start[11..13];
                    *hour_counts.entry(format!("{}:00", hour)).or_insert(0) += 1;
                }
            }
        }
    }

    let mut data: Vec<serde_json::Value> = hour_counts
        .into_iter()
        .map(|(k, v)| serde_json::json!({"label": k, "value": v}))
        .collect();
    data.sort_by(|a, b| a["label"].as_str().cmp(&b["label"].as_str()));

    Ok(serde_json::json!({
        "dashboard_id": "encounters_today",
        "chart_type": "bar",
        "total_today": total_today,
        "datasets": [
            {"title": "Encounters by Hour", "data": data},
        ]
    }))
}

async fn run_recent_activity(state: &AppState) -> Result<serde_json::Value, String> {
    let encounters = get_resources(state, "Encounter").await;

    // Build patient lookup
    let patients = get_resources(state, "Patient").await;
    let mut patient_names: HashMap<String, String> = HashMap::new();
    for p in &patients {
        let id = p.get("id").and_then(|i| i.as_str()).unwrap_or("").to_string();
        let name = p
            .get("name")
            .and_then(|n| n.as_array())
            .and_then(|arr| arr.first())
            .and_then(|n| n.get("text"))
            .and_then(|t| t.as_str())
            .unwrap_or("Unknown")
            .to_string();
        patient_names.insert(id, name);
    }

    // Sort encounters by date (newest first), take 20
    let mut sorted_encounters: Vec<&serde_json::Value> = encounters.iter().collect();
    sorted_encounters.sort_by(|a, b| {
        let a_date = a.get("period").and_then(|p| p.get("start")).and_then(|s| s.as_str()).unwrap_or("");
        let b_date = b.get("period").and_then(|p| p.get("start")).and_then(|s| s.as_str()).unwrap_or("");
        b_date.cmp(a_date)
    });

    let rows: Vec<serde_json::Value> = sorted_encounters
        .iter()
        .take(20)
        .map(|e| {
            let date = e.get("period").and_then(|p| p.get("start")).and_then(|s| s.as_str()).unwrap_or("—");
            let patient_ref = e.get("subject").and_then(|s| s.get("reference")).and_then(|r| r.as_str()).unwrap_or("");
            let patient_id = patient_ref.strip_prefix("Patient/").unwrap_or(patient_ref);
            let patient_name = patient_names.get(patient_id).cloned().unwrap_or_else(|| "Unknown".to_string());
            let reason = e
                .get("reasonCode")
                .and_then(|r| r.as_array())
                .and_then(|arr| arr.first())
                .and_then(|r| r.get("text"))
                .and_then(|t| t.as_str())
                .unwrap_or("—");
            let status = e.get("status").and_then(|s| s.as_str()).unwrap_or("—");

            serde_json::json!({
                "date": date,
                "patient": patient_name,
                "reason": reason,
                "status": status,
            })
        })
        .collect();

    Ok(serde_json::json!({
        "dashboard_id": "recent_activity",
        "chart_type": "table",
        "columns": ["Date", "Patient", "Reason", "Status"],
        "rows": rows,
    }))
}
