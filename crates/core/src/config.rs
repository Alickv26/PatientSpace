use serde::Deserialize;

#[derive(Debug, Deserialize, Clone)]
pub struct Config {
    pub facility_id: String,
    pub bind: String,
    pub database_url: String,
    pub hub_url: String,
}

impl Config {
    pub fn load() -> anyhow::Result<Self> {
        // Try PATIENTSPACE_ env vars first, then fall back to defaults.
        let facility_id = std::env::var("PATIENTSPACE_FACILITY_ID")
            .unwrap_or_else(|_| "F-UNKNOWN".to_string());
        let bind = std::env::var("PATIENTSPACE_BIND")
            .unwrap_or_else(|_| "127.0.0.1:8080".to_string());
        let database_url = std::env::var("PATIENTSPACE_DATABASE_URL")
            .unwrap_or_else(|_| "sqlite:patientspace.db?mode=rwc".to_string());
        let hub_url = std::env::var("PATIENTSPACE_HUB_URL")
            .unwrap_or_else(|_| "https://hub.patientspace.org".to_string());

        Ok(Self {
            facility_id,
            bind,
            database_url,
            hub_url,
        })
    }
}
