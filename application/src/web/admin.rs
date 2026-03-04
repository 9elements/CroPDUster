//! Admin-only REST API handlers.

use picoserve::extract::Json as JsonBody;
use picoserve::response::{Json, StatusCode};
use serde::{Deserialize, Serialize};

use crate::auth::{hash_password, AdminUser, AuthUser};
use crate::storage::{port_name_key, seed_defaults, user_admin_key, user_ports_key, user_pw_key};

// ── Response types ─────────────────────────────────────────────────────────────

#[derive(Serialize)]
pub struct OkResponse {
    pub ok: bool,
}

/// Returns `Ok(Json(OkResponse { ok: true }))` or `Err((StatusCode, &'static str))`.
/// `Result<Json<OkResponse>, (StatusCode, &'static str)>: IntoResponse` ✓
type ApiResult = Result<Json<OkResponse>, (StatusCode, &'static str)>;

const fn api_ok() -> ApiResult {
    Ok(Json(OkResponse { ok: true }))
}

fn api_err(code: StatusCode, msg: &'static str) -> ApiResult {
    Err((code, msg))
}

// ── Port name ──────────────────────────────────────────────────────────────────

#[derive(Deserialize)]
pub struct PortNameBody {
    pub name: heapless::String<16>,
}

#[derive(Serialize)]
pub struct PortNameResponse {
    pub name: heapless::String<16>,
}

pub async fn handle_port_name_set(
    port: u8,
    _admin: AdminUser,
    JsonBody(body): JsonBody<PortNameBody>,
) -> ApiResult {
    if port >= 8 {
        return api_err(StatusCode::BAD_REQUEST, "Invalid port");
    }
    let key = port_name_key(port);
    let db = crate::web::db();
    let mut wtx = db.write_transaction().await;
    wtx.write(&key, body.name.as_bytes())
        .await
        .map_err(|_| (StatusCode::INTERNAL_SERVER_ERROR, "Write error"))?;
    wtx.commit()
        .await
        .map_err(|_| (StatusCode::INTERNAL_SERVER_ERROR, "Commit error"))?;
    api_ok()
}

pub async fn handle_port_name_get(port: u8) -> Json<PortNameResponse> {
    let key = port_name_key(port);
    let mut buf = [0u8; 16];
    let db = crate::web::db();
    let rtx = db.read_transaction().await;
    let name = if let Ok(n) = rtx.read(&key, &mut buf).await {
        heapless::String::from_utf8(heapless::Vec::from_slice(&buf[..n]).unwrap_or_default())
            .unwrap_or_default()
    } else {
        heapless::String::new()
    };
    Json(PortNameResponse { name })
}

// ── Password change ────────────────────────────────────────────────────────────

#[derive(Deserialize)]
pub struct ChangePasswordBody {
    pub old: heapless::String<64>,
    pub new: heapless::String<64>,
}

pub async fn handle_password_change(
    user: AuthUser,
    JsonBody(body): JsonBody<ChangePasswordBody>,
) -> ApiResult {
    let old_hash = hash_password(body.old.as_str());
    let pw_key = user_pw_key(user.username.as_str());
    let db = crate::web::db();
    let mut stored = [0u8; 32];
    {
        let rtx = db.read_transaction().await;
        let n = rtx
            .read(&pw_key, &mut stored)
            .await
            .map_err(|_| (StatusCode::INTERNAL_SERVER_ERROR, "Storage error"))?;
        if n != 32 {
            return api_err(StatusCode::UNAUTHORIZED, "Invalid credentials");
        }
    }
    if stored != old_hash {
        return api_err(StatusCode::UNAUTHORIZED, "Invalid credentials");
    }
    let new_hash = hash_password(body.new.as_str());
    let mut wtx = db.write_transaction().await;
    wtx.write(&pw_key, &new_hash)
        .await
        .map_err(|_| (StatusCode::INTERNAL_SERVER_ERROR, "Write error"))?;
    wtx.commit()
        .await
        .map_err(|_| (StatusCode::INTERNAL_SERVER_ERROR, "Commit error"))?;
    api_ok()
}

// ── Create user ────────────────────────────────────────────────────────────────

#[derive(Deserialize)]
pub struct CreateUserBody {
    pub username: heapless::String<32>,
    pub password: heapless::String<64>,
    pub ports: u8,
    pub admin: bool,
}

pub async fn handle_create_user(
    _admin: AdminUser,
    JsonBody(body): JsonBody<CreateUserBody>,
) -> ApiResult {
    let pw_hash = hash_password(body.password.as_str());
    let pw_key = user_pw_key(body.username.as_str());
    let admin_key = user_admin_key(body.username.as_str());
    let ports_key = user_ports_key(body.username.as_str());

    // Keys MUST be in sorted order: u/{name}/admin < u/{name}/ports < u/{name}/pw
    let db = crate::web::db();
    let mut wtx = db.write_transaction().await;
    wtx.write(&admin_key, if body.admin { b"1" } else { b"0" })
        .await
        .map_err(|_| (StatusCode::INTERNAL_SERVER_ERROR, "Write error"))?;
    wtx.write(&ports_key, &[body.ports])
        .await
        .map_err(|_| (StatusCode::INTERNAL_SERVER_ERROR, "Write error"))?;
    wtx.write(&pw_key, &pw_hash)
        .await
        .map_err(|_| (StatusCode::INTERNAL_SERVER_ERROR, "Write error"))?;
    wtx.commit()
        .await
        .map_err(|_| (StatusCode::INTERNAL_SERVER_ERROR, "Commit error"))?;
    api_ok()
}

// ── Set user ports ─────────────────────────────────────────────────────────────

#[derive(Deserialize)]
pub struct SetPortsBody {
    pub ports: u8,
}

pub async fn handle_set_user_ports(
    username: heapless::String<32>,
    _admin: AdminUser,
    JsonBody(body): JsonBody<SetPortsBody>,
) -> ApiResult {
    let ports_key = user_ports_key(username.as_str());
    let db = crate::web::db();
    let mut wtx = db.write_transaction().await;
    wtx.write(&ports_key, &[body.ports])
        .await
        .map_err(|_| (StatusCode::INTERNAL_SERVER_ERROR, "Write error"))?;
    wtx.commit()
        .await
        .map_err(|_| (StatusCode::INTERNAL_SERVER_ERROR, "Commit error"))?;
    api_ok()
}

// ── Delete user ────────────────────────────────────────────────────────────────

pub async fn handle_delete_user(
    username: heapless::String<32>,
    _admin: AdminUser,
) -> ApiResult {
    let pw_key = user_pw_key(username.as_str());
    let ports_key = user_ports_key(username.as_str());
    let admin_key = user_admin_key(username.as_str());
    // Sorted: admin < ports < pw
    let db = crate::web::db();
    let mut wtx = db.write_transaction().await;
    // Ignore individual write errors — deletion is best-effort
    let _ = wtx.write(&admin_key, b"0").await;
    let _ = wtx.write(&ports_key, &[0u8]).await;
    let _ = wtx.write(&pw_key, &[0u8; 32]).await;
    wtx.commit()
        .await
        .map_err(|_| (StatusCode::INTERNAL_SERVER_ERROR, "Commit error"))?;
    api_ok()
}

// ── Factory reset ──────────────────────────────────────────────────────────────

pub async fn handle_factory_reset(_admin: AdminUser) -> Json<OkResponse> {
    let db = crate::web::db();
    db.format().await.ok();
    seed_defaults(db).await;
    embassy_time::Timer::after_millis(50).await;
    cortex_m::peripheral::SCB::sys_reset();
}
