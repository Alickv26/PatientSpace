//! `patientspace migrate` CLI — migrates SmartCare data into PatientSpace.
//!
//! Supports three source types:
//!   - `fhir` — reads from SmartCare Pro HIE FHIR endpoint (preferred)
//!   - `db`   — reads from SmartCare Pro MySQL dump (CSV files)
//!   - `csv`  — reads from SmartCare Legacy/Plus CSV export
//!
//! Usage:
//!   patientspace migrate mock-server --count 100
//!   patientspace migrate run --source fhir --source-url http://127.0.0.1:9090/fhir --target http://localhost:8080
//!   patientspace migrate run --source db --source-url /path/to/dump.csv --target http://localhost:8080
//!   patientspace migrate run --source csv --source-url /path/to/patients.csv --target http://localhost:8080
//!   patientspace migrate run --source auto --source-url http://127.0.0.1:9090/fhir --target http://localhost:8080
//!   patientspace migrate run --dry-run --source fhir --source-url http://127.0.0.1:9090/fhir
//!   patientspace migrate validate --smartcare-count 100 --patientspace-url http://localhost:8080

use clap::{Parser, Subcommand};
use tracing::{info, warn};

use patientspace_smartcare_adapter::SmartCareIngestAdapter;

#[derive(Parser)]
#[command(name = "patientspace-migrate")]
#[command(about = "Migrate SmartCare data into PatientSpace — supports FHIR, DB, and CSV sources")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Start a mock SmartCare HIE server (for testing).
    MockServer {
        #[arg(long, default_value = "127.0.0.1:9090")]
        bind: String,
        #[arg(long, default_value_t = 100)]
        count: u32,
    },
    /// Migrate data from SmartCare to PatientSpace.
    Run {
        /// Source type: fhir, db, csv, or auto (tries fhir → db → csv)
        #[arg(long, default_value = "auto")]
        source: String,
        /// Source URL or file path (depends on source type).
        #[arg(long, default_value = "http://127.0.0.1:9090/fhir")]
        source_url: String,
        /// PatientSpace core URL.
        #[arg(long, default_value = "http://localhost:8080")]
        target_url: String,
        /// Dry-run: count and validate without ingesting.
        #[arg(long)]
        dry_run: bool,
    },
    /// Validate a completed migration.
    Validate {
        #[arg(long)]
        smartcare_count: u32,
        #[arg(long, default_value = "http://localhost:8080")]
        patientspace_url: String,
    },
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    let cli = Cli::parse();

    match cli.command {
        Commands::MockServer { bind, count } => {
            info!(%bind, count, "starting mock SmartCare HIE server");
            let config = patientspace_smartcare_adapter::mock_server::MockServerConfig {
                bind,
                num_patients: count,
            };
            patientspace_smartcare_adapter::mock_server::start_server(config).await?;
        }
        Commands::Run {
            source,
            source_url,
            target_url,
            dry_run,
        } => {
            info!(%source, %source_url, %target_url, dry_run, "starting migration");

            // Create the adapter based on source type.
            let mut adapter = create_adapter(&source, &source_url)?;

            // Count + ingest.
            let mut source_count = 0u32;
            let mut patients = Vec::new();

            for result in adapter.iter_patients() {
                match result {
                    Ok(patient) => {
                        source_count += 1;
                        if dry_run {
                            if source_count % 10 == 0 {
                                info!(count = source_count, "dry-run: counted patients");
                            }
                        } else {
                            patients.push(patient);
                        }
                    }
                    Err(e) => {
                        warn!(error = %e, "failed to read patient from source");
                    }
                }
            }

            info!(source_count, "source has {} patients", source_count);

            if dry_run {
                println!("\n=== Dry Run Report ===");
                println!("Source type:     {}", source);
                println!("Source URL:      {}", source_url);
                println!("Source patients: {}", source_count);
                println!("No data was ingested.");
                return Ok(());
            }

            // Ingest each patient via the PatientSpace FHIR API.
            let http = reqwest::Client::new();
            let mut ingested = 0u32;
            let mut failed = 0u32;

            for patient in patients {
                let json = serde_json::to_value(&patient)?;
                let resp = http
                    .post(format!("{}/fhir/Patient", target_url))
                    .header("Content-Type", "application/json")
                    .json(&json)
                    .send()
                    .await
                    .map_err(|e| anyhow::anyhow!("HTTP error: {e}"))?;

                if resp.status().is_success() {
                    ingested += 1;
                    if ingested % 10 == 0 {
                        info!(ingested, "ingested {} patients", ingested);
                    }
                } else {
                    failed += 1;
                    warn!(status = resp.status().to_string(), "failed to ingest patient");
                }
            }

            // Validation report.
            let report = patientspace_smartcare_adapter::ValidationReport::new(
                source_count,
                ingested,
                failed,
            );

            println!("\n{}", report.to_json());
            println!("\nSource type: {}", source);

            if report.status == patientspace_smartcare_adapter::validation::ValidationStatus::Fail {
                std::process::exit(1);
            }
        }
        Commands::Validate {
            smartcare_count,
            patientspace_url,
        } => {
            info!(smartcare_count, %patientspace_url, "validating migration");

            let resp = reqwest::get(format!("{}/fhir/Patient", patientspace_url))
                .await
                .map_err(|e| anyhow::anyhow!("HTTP error: {e}"))?;

            let bundle: serde_json::Value = resp.json().await?;
            let patientspace_count = bundle
                .get("total")
                .and_then(|v| v.as_u64())
                .unwrap_or(0) as u32;

            let report = patientspace_smartcare_adapter::ValidationReport::new(
                smartcare_count,
                patientspace_count,
                0,
            );

            println!("\n{}", report.to_json());
        }
    }

    Ok(())
}

/// Create the appropriate adapter based on the source type.
/// With `auto`, tries FHIR first, then DB, then CSV.
fn create_adapter(
    source: &str,
    source_url: &str,
) -> anyhow::Result<Box<dyn SmartCareIngestAdapter>> {
    match source {
        "fhir" => {
            info!(%source_url, "using FHIR adapter");
            let config = patientspace_smartcare_adapter::FhirAdapterConfig {
                base_url: source_url.to_string(),
                bearer_token: String::new(),
                page_size: 100,
            };
            Ok(Box::new(patientspace_smartcare_adapter::SmartCareFhirAdapter::new(config)))
        }
        "db" => {
            info!(%source_url, "using DB adapter (reads CSV dump from path)");
            let config = patientspace_smartcare_adapter::DbAdapterConfig {
                connection_string: source_url.to_string(),
                query: String::new(),
            };
            Ok(Box::new(patientspace_smartcare_adapter::SmartCareDbAdapter::new(config)))
        }
        "csv" => {
            info!(%source_url, "using CSV adapter");
            let config = patientspace_smartcare_adapter::CsvAdapterConfig {
                file_path: source_url.to_string(),
                has_header: true,
            };
            Ok(Box::new(patientspace_smartcare_adapter::SmartCareCsvAdapter::new(config)))
        }
        "auto" => {
            // Try FHIR first (if source_url starts with http).
            if source_url.starts_with("http") {
                info!(%source_url, "auto-detection: trying FHIR adapter");
                let config = patientspace_smartcare_adapter::FhirAdapterConfig {
                    base_url: source_url.to_string(),
                    bearer_token: String::new(),
                    page_size: 100,
                };
                return Ok(Box::new(patientspace_smartcare_adapter::SmartCareFhirAdapter::new(config)));
            }

            // Try DB dump (if path ends in .csv or is a directory).
            let path = std::path::Path::new(source_url);
            if path.exists() && (path.is_file() || path.is_dir()) {
                info!(%source_url, "auto-detection: using DB adapter (file/dir found)");
                let config = patientspace_smartcare_adapter::DbAdapterConfig {
                    connection_string: source_url.to_string(),
                    query: String::new(),
                };
                return Ok(Box::new(patientspace_smartcare_adapter::SmartCareDbAdapter::new(config)));
            }

            anyhow::bail!("auto-detection failed: source_url is not HTTP and not a file/directory: {}", source_url);
        }
        other => anyhow::bail!("unknown source type: {} (use fhir, db, csv, or auto)", other),
    }
}
