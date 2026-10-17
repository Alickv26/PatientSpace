//! Authentication — clinician login, sessions, and password hashing.
//!
//! Uses argon2 for password hashing and random session tokens stored in SQLite.
//! Session cookies are HttpOnly + SameSite=Strict.
//!
//! On login, a delegated Ed25519 signing keypair is generated (if the clinician
//! doesn't already have one) and stored. This key is used to co-sign every
//! PSEP envelope the clinician creates, so every resource can be traced back
//! to the individual who authored it — not just the facility.

use argon2::{
    password_hash::{PasswordHasher, SaltString},
    Argon2, PasswordHash, PasswordVerifier,
};
use axum::extract::State;
use axum::http::{HeaderMap, StatusCode, header::SET_COOKIE};
use axum::response::IntoResponse;
use axum::Json;
use base64::Engine;
use ed25519_dalek::SigningKey;
use serde::{Deserialize, Serialize};
use sqlx::Row;
use tracing::info;
use uuid::Uuid;

use crate::http::AppState;

const SESSION_DURATION_SECS: i64 = 8 * 3600; // 8 hours

#[derive(Debug, Serialize)]
pub struct Clinician {
    pub id: String,
    pub username: String,
    pub full_name: String,
    pub role: String,
}

#[derive(Debug, Deserialize)]
pub struct LoginRequest {
    pub username: String,
    pub password: String,
}

#[derive(Debug, Serialize)]
pub struct LoginResponse {
    pub clinician: Clinician,
    pub session_token: String,
}

/// Information about the logged-in clinician, available to HTTP handlers
/// for co-signing PSEP envelopes.
#[derive(Debug, Clone)]
pub struct ClinicianContext {
    pub clinician_id: String,
    pub signing_key: SigningKey,
}

/// Extract session token from Cookie header.
fn extract_session_token(headers: &HeaderMap) -> Option<String> {
    let cookie_header = headers.get("cookie")?.to_str().ok()?;
    for cookie in cookie_header.split(';') {
        let cookie = cookie.trim();
        if let Some(token) = cookie.strip_prefix("patientspace_session=") {
            return Some(token.to_string());
        }
    }
    None
}

/// POST /api/auth/login
pub async fn login(
    State(state): State<AppState>,
    body: axum::body::Bytes,
) -> impl IntoResponse {
    let req: LoginRequest = match serde_json::from_slice(&body) {
        Ok(r) => r,
        Err(e) => return (StatusCode::BAD_REQUEST, Json(serde_json::json!({"error": format!("invalid JSON: {e}")}))).into_response(),
    };

    let row = sqlx::query(
        "SELECT id, username, full_name, role, password_hash, active, signing_key_priv, signing_key_pub FROM clinicians WHERE username = ?",
    )
    .bind(&req.username)
    .fetch_optional(state.log.pool())
    .await;

    let row = match row {
        Ok(Some(r)) => r,
        _ => {
            return (StatusCode::UNAUTHORIZED, Json(serde_json::json!({"error": "Invalid username or password"}))).into_response();
        }
    };

    let active: i64 = row.try_get("active").unwrap_or(0);
    if active != 1 {
        return (StatusCode::UNAUTHORIZED, Json(serde_json::json!({"error": "Account disabled"}))).into_response();
    }

    let password_hash: String = row.try_get("password_hash").unwrap_or_default();
    let parsed_hash = match PasswordHash::new(&password_hash) {
        Ok(h) => h,
        Err(_) => {
            return (StatusCode::UNAUTHORIZED, Json(serde_json::json!({"error": "Invalid username or password"}))).into_response();
        }
    };

    if Argon2::default()
        .verify_password(req.password.as_bytes(), &parsed_hash)
        .is_err()
    {
        return (StatusCode::UNAUTHORIZED, Json(serde_json::json!({"error": "Invalid username or password"}))).into_response();
    }

    let clinician = Clinician {
        id: row.try_get("id").unwrap_or_default(),
        username: row.try_get("username").unwrap_or_default(),
        full_name: row.try_get("full_name").unwrap_or_default(),
        role: row.try_get("role").unwrap_or_default(),
    };

    let signing_key = match get_or_create_signing_key(state.log.pool(), &clinician.id).await {
        Ok(k) => k,
        Err(e) => {
            return (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"error": format!("Failed to get signing key: {e}")}))).into_response();
        }
    };

    let signing_priv_hex = hex::encode(signing_key.to_bytes());
    let signing_pub_hex = hex::encode(signing_key.verifying_key().to_bytes());

    let token = generate_session_token();
    let now = chrono::Utc::now().timestamp();
    let expires = now + SESSION_DURATION_SECS;

    if let Err(e) = sqlx::query(
        "INSERT INTO sessions (token, clinician_id, created_at, expires_at, signing_key_pub, signing_key_priv) VALUES (?, ?, ?, ?, ?, ?)",
    )
    .bind(&token)
    .bind(&clinician.id)
    .bind(now)
    .bind(expires)
    .bind(&signing_pub_hex)
    .bind(&signing_priv_hex)
    .execute(state.log.pool())
    .await
    {
        return (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"error": format!("Failed to create session: {e}")}))).into_response();
    }

    info!(clinician_id = %clinician.id, "clinician logged in");

    let cookie = format!(
        "patientspace_session={}; Path=/; HttpOnly; SameSite=Strict; Max-Age={}",
        token, SESSION_DURATION_SECS
    );

    let login_resp = serde_json::json!({
        "clinician": {
            "id": clinician.id,
            "username": clinician.username,
            "full_name": clinician.full_name,
            "role": clinician.role,
        },
        "session_token": token,
    });

    let mut response = (StatusCode::OK, Json(login_resp)).into_response();
    response.headers_mut().insert(SET_COOKIE, cookie.parse().unwrap());
    response
}

/// POST /api/auth/logout
pub async fn logout(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> impl IntoResponse {
    if let Some(token) = extract_session_token(&headers) {
        let _ = sqlx::query("DELETE FROM sessions WHERE token = ?")
            .bind(&token)
            .execute(state.log.pool())
            .await;
    }

    let cookie = "patientspace_session=; Path=/; HttpOnly; SameSite=Strict; Max-Age=0";

    (
        StatusCode::OK,
        [(SET_COOKIE, cookie)],
        Json(serde_json::json!({"status": "logged out"})),
    )
}

/// GET /api/auth/me
pub async fn me(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> impl IntoResponse {
    let token = match extract_session_token(&headers) {
        Some(t) => t,
        None => {
            return (
                StatusCode::UNAUTHORIZED,
                Json(serde_json::json!({"error": "Not authenticated"})),
            );
        }
    };

    match get_clinician_from_session(state.log.pool(), &token).await {
        Ok(Some(clinician)) => (StatusCode::OK, Json(serde_json::json!(clinician))),
        _ => (
            StatusCode::UNAUTHORIZED,
            Json(serde_json::json!({"error": "Not authenticated"})),
        ),
    }
}

/// Verify a session token and return the clinician + signing key.
/// Called by write handlers to co-sign PSEP envelopes.
pub async fn get_clinician_context(
    pool: &sqlx::SqlitePool,
    token: &str,
) -> Result<Option<ClinicianContext>, sqlx::Error> {
    let now = chrono::Utc::now().timestamp();

    let row = sqlx::query(
        r#"SELECT s.clinician_id, s.signing_key_priv
           FROM sessions s
           JOIN clinicians c ON c.id = s.clinician_id
           WHERE s.token = ? AND s.expires_at > ? AND c.active = 1"#,
    )
    .bind(token)
    .bind(now)
    .fetch_optional(pool)
    .await?;

    match row {
        Some(r) => {
            let clinician_id: String = r.try_get("clinician_id").unwrap_or_default();
            let signing_key_hex: String = r.try_get("signing_key_priv").unwrap_or_default();

            // Parse the signing key from hex.
            let signing_key_bytes = match hex::decode(&signing_key_hex) {
                Ok(b) if b.len() == 32 => {
                    let mut arr = [0u8; 32];
                    arr.copy_from_slice(&b);
                    SigningKey::from_bytes(&arr)
                }
                _ => {
                    // Fallback: generate a new key if the stored one is corrupt.
                    let mut csprng = rand::rng();
                    SigningKey::generate(&mut csprng)
                }
            };

            Ok(Some(ClinicianContext {
                clinician_id,
                signing_key: signing_key_bytes,
            }))
        }
        None => Ok(None),
    }
}

/// Get the clinician context from request headers (cookie).
/// Returns None if not authenticated.
pub async fn get_clinician_from_headers(
    pool: &sqlx::SqlitePool,
    headers: &HeaderMap,
) -> Option<ClinicianContext> {
    let token = extract_session_token(headers)?;
    get_clinician_context(pool, &token).await.ok().flatten()
}

/// Verify a session token and return the clinician info (for /me endpoint).
pub async fn get_clinician_from_session(
    pool: &sqlx::SqlitePool,
    token: &str,
) -> Result<Option<Clinician>, sqlx::Error> {
    let now = chrono::Utc::now().timestamp();

    let row = sqlx::query(
        r#"SELECT c.id, c.username, c.full_name, c.role
           FROM sessions s
           JOIN clinicians c ON c.id = s.clinician_id
           WHERE s.token = ? AND s.expires_at > ? AND c.active = 1"#,
    )
    .bind(token)
    .bind(now)
    .fetch_optional(pool)
    .await?;

    Ok(row.map(|r| Clinician {
        id: r.try_get("id").unwrap_or_default(),
        username: r.try_get("username").unwrap_or_default(),
        full_name: r.try_get("full_name").unwrap_or_default(),
        role: r.try_get("role").unwrap_or_default(),
    }))
}

/// Create a default admin clinician on first boot if no clinicians exist.
pub async fn ensure_default_admin(pool: &sqlx::SqlitePool) -> anyhow::Result<()> {
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM clinicians")
        .fetch_one(pool)
        .await?;

    if count > 0 {
        return Ok(());
    }

    info!("No clinicians found — creating default admin user");

    // Generate salt using base64-encoded random bytes (avoids rand_core version conflict).
    let mut salt_bytes = [0u8; 16];
    rand::fill(&mut salt_bytes);
    let salt_b64 = base64::engine::general_purpose::STANDARD_NO_PAD.encode(salt_bytes);
    let salt = SaltString::from_b64(&salt_b64)
        .unwrap_or_else(|_| SaltString::from_b64("AAAAAAAAAAAAAA").unwrap());
    let argon2 = Argon2::default();
    let hash = argon2
        .hash_password(b"admin123", &salt)
        .map_err(|e| anyhow::anyhow!("argon2 hash failed: {e}"))?
        .to_string();

    // Generate the admin's signing key.
    let mut csprng = rand::rng();
    let signing_key = SigningKey::generate(&mut csprng);
    let signing_pub = hex::encode(signing_key.verifying_key().to_bytes());
    let signing_priv = hex::encode(signing_key.to_bytes());

    let id = Uuid::now_v7().to_string();
    let now = chrono::Utc::now().timestamp();

    sqlx::query(
        "INSERT INTO clinicians (id, username, full_name, role, password_hash, created_at, active, signing_key_pub, signing_key_priv) VALUES (?, ?, ?, ?, ?, ?, 1, ?, ?)",
    )
    .bind(&id)
    .bind("admin")
    .bind("System Administrator")
    .bind("admin")
    .bind(&hash)
    .bind(now)
    .bind(&signing_pub)
    .bind(&signing_priv)
    .execute(pool)
    .await?;

    info!("Default admin created — username: admin, password: admin123");
    Ok(())
}

/// Get an existing signing key or generate a new one for a clinician.
async fn get_or_create_signing_key(
    pool: &sqlx::SqlitePool,
    clinician_id: &str,
) -> anyhow::Result<SigningKey> {
    // Check if the clinician already has a key.
    let row = sqlx::query("SELECT signing_key_priv FROM clinicians WHERE id = ?")
        .bind(clinician_id)
        .fetch_optional(pool)
        .await?;

    if let Some(r) = row {
        let key_hex: String = r.try_get("signing_key_priv").unwrap_or_default();
        if !key_hex.is_empty() {
            if let Ok(bytes) = hex::decode(&key_hex) {
                if bytes.len() == 32 {
                    let mut arr = [0u8; 32];
                    arr.copy_from_slice(&bytes);
                    return Ok(SigningKey::from_bytes(&arr));
                }
            }
        }
    }

    // Generate a new key (scope the RNG so it's dropped before the .await).
    let (signing_key, signing_pub, signing_priv) = {
        let mut csprng = rand::rng();
        let signing_key = SigningKey::generate(&mut csprng);
        let signing_pub = hex::encode(signing_key.verifying_key().to_bytes());
        let signing_priv = hex::encode(signing_key.to_bytes());
        (signing_key, signing_pub, signing_priv)
    };

    sqlx::query("UPDATE clinicians SET signing_key_pub = ?, signing_key_priv = ? WHERE id = ?")
        .bind(&signing_pub)
        .bind(&signing_priv)
        .bind(clinician_id)
        .execute(pool)
        .await?;

    info!(clinician_id, "generated new signing key for clinician");

    Ok(signing_key)
}

fn generate_session_token() -> String {
    let bytes: [u8; 32] = std::iter::repeat_with(|| {
        use std::collections::hash_map::RandomState;
        use std::hash::{BuildHasher, Hasher};
        RandomState::new().build_hasher().finish() as u8
    })
    .collect::<Vec<_>>()
    .try_into()
    .unwrap_or([0u8; 32]);
    hex::encode(bytes)
}
