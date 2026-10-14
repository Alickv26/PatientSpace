//! Point-of-care analytics — embedded DuckDB dashboards.
//!
//! This is the headline differentiator vs SmartCare: every clinician's screen
//! has live dashboards backed by DuckDB running inside the same Rust binary
//! as the EHR.
//!
//! v0.1: dashboard definitions only. DuckDB wiring lands in `core`.

use serde::{Deserialize, Serialize};

/// A named dashboard, backed by a SQL query (DuckDB dialect).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Dashboard {
    pub id: String,
    pub title: String,
    pub description: String,
    /// SQL query in DuckDB dialect. Receives `:facility_id`, `:clinician_id`,
    /// `:start_date`, `:end_date` as named parameters.
    pub sql: String,
    /// Visualization hints for the frontend.
    pub visualization: Visualization,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Visualization {
    Table { columns: Vec<String> },
    BarChart { x: String, y: String },
    LineChart { x: String, y: String },
    PieChart { category: String, value: String },
    Kpi { label: String, value_column: String },
}

/// Built-in dashboards shipped with v0.1.
pub fn builtin_dashboards() -> Vec<Dashboard> {
    vec![
        Dashboard {
            id: "clinic_today".to_string(),
            title: "Clinic Today".to_string(),
            description: "Queue length, avg wait, suspected TB cases flagged, defaulters on ART walked in today.".to_string(),
            sql: r#"
                SELECT
                  date_trunc('hour', e.period_start) AS hour,
                  count(*) AS visits,
                  count(*) FILTER (WHERE e.status = 'arrived' AND e.period_end IS NULL) AS in_queue
                FROM encounter e
                WHERE e.period_start::date = current_date
                  AND e.facility_id = :facility_id
                GROUP BY 1
                ORDER BY 1
            "#.to_string(),
            visualization: Visualization::BarChart {
                x: "hour".to_string(),
                y: "visits".to_string(),
            },
        },
        Dashboard {
            id: "art_defaulters".to_string(),
            title: "ART Defaulters (30+ days overdue)".to_string(),
            description: "Patients on ART who have missed their expected pickup date by 30+ days.".to_string(),
            sql: r#"
                SELECT
                  p.id AS patient_id,
                  p.name_text,
                  p.smartcard_no,
                  max(med.authored_on) AS last_dispense,
                  current_date - max(med.authored_on)::date AS days_since_last_dispense
                FROM medication_request med
                JOIN patient p ON p.id = med.subject
                WHERE med.medication_code LIKE 'J05%'  -- WHO ATC: antivirals
                  AND med.facility_id = :facility_id
                GROUP BY 1, 2, 3
                HAVING max(med.authored_on) < current_date - INTERVAL '30 days'
                ORDER BY days_since_last_dispense DESC
            "#.to_string(),
            visualization: Visualization::Table {
                columns: vec![
                    "patient_id".to_string(),
                    "name_text".to_string(),
                    "smartcard_no".to_string(),
                    "last_dispense".to_string(),
                    "days_since_last_dispense".to_string(),
                ],
            },
        },
        Dashboard {
            id: "hiv_viral_suppression".to_string(),
            title: "HIV Viral Suppression Rate (24 months)".to_string(),
            description: "Percentage of patients with viral load < 1000 copies/mL in the last 24 months.".to_string(),
            sql: r#"
                WITH vl AS (
                  SELECT
                    date_trunc('month', o.effective_date) AS month,
                    count(*) FILTER (WHERE o.value_numeric < 1000) AS suppressed,
                    count(*) AS total
                  FROM observation o
                  WHERE o.code = '856-6'  -- LOINC: HIV 1 RNA viral load
                    AND o.facility_id = :facility_id
                    AND o.effective_date > current_date - INTERVAL '24 months'
                  GROUP BY 1
                )
                SELECT
                  month,
                  suppressed,
                  total,
                  round(100.0 * suppressed / nullif(total, 0), 1) AS suppression_pct
                FROM vl
                ORDER BY month
            "#.to_string(),
            visualization: Visualization::LineChart {
                x: "month".to_string(),
                y: "suppression_pct".to_string(),
            },
        },
    ]
}
