//! Authentication — clinician login, sessions, and password hashing.
//!
//! Uses argon2 for password hashing and random session tokens stored in SQLite.
//! Session cookies are HttpOnly + SameSite=Strict.

use argon2::{
    password_hash::{rand_core::OsRng, PasswordHasher, SaltString},
    Argon2, PasswordHash, PasswordVerifier,
};
use axum::extract::State;
use axum::http::{HeaderMap, StatusCode, header::SET_COOKIE};
use axum::response::IntoResponse;
use axum::Json;
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
    Json(req): Json<LoginRequest>,
) -> axum::response::Response {
    use axum::response::IntoResponse;

    let row = sqlx::query(
        "SELECT id, username, full_name, role, password_hash, active FROM clinicians WHERE username = ?",
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

    let token = generate_session_token();
    let now = chrono::Utc::now().timestamp();
    let expires = now + SESSION_DURATION_SECS;

    if let Err(e) = sqlx::query(
        "INSERT INTO sessions (token, clinician_id, created_at, expires_at) VALUES (?, ?, ?, ?)",
    )
    .bind(&token)
    .bind(&clinician.id)
    .bind(now)
    .bind(expires)
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

    let mut resp = (StatusCode::OK, Json(LoginResponse {
        clinician,
        session_token: token,
    })).into_response();
    resp.headers_mut().insert(SET_COOKIE, cookie.parse().unwrap());
    resp
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

/// Verify a session token and return the clinician.
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

/// Create a default admin clinician on first boot if none exist.
pub async fn ensure_default_admin(pool: &sqlx::SqlitePool) -> anyhow::Result<()> {
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM clinicians")
        .fetch_one(pool)
        .await?;

    if count > 0 {
        return Ok(());
    }

    info!("No clinicians found — creating default admin user");

    let salt = SaltString::generate(&mut OsRng);
    let argon2 = Argon2::default();
    let hash = argon2
        .hash_password(b"admin123", &salt)
        .map_err(|e| anyhow::anyhow!("argon2 hash failed: {e}"))?
        .to_string();

    let id = Uuid::now_v7().to_string();
    let now = chrono::Utc::now().timestamp();

    sqlx::query(
        "INSERT INTO clinicians (id, username, full_name, role, password_hash, created_at, active) VALUES (?, ?, ?, ?, ?, ?, 1)",
    )
    .bind(&id)
    .bind("admin")
    .bind("System Administrator")
    .bind("admin")
    .bind(&hash)
    .bind(now)
    .execute(pool)
    .await?;

    info!("Default admin created — username: admin, password: admin123");
    Ok(())
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
