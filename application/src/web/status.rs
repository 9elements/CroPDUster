//! GET /api/status handler.

use picoserve::response::Json;
use serde::Serialize;

use crate::storage::KEY_ADMIN_FIRST_LOGIN;

#[derive(Serialize)]
pub struct StatusResponse {
    pub version: &'static str,
    pub first_login: bool,
}

/// `GET /api/status` — returns version and first-login flag.
pub async fn handle_status() -> Json<StatusResponse> {
    let db = crate::web::db();
    let mut buf = [0u8; 1];
    let first_login = {
        let rtx = db.read_transaction().await;
        match rtx.read(KEY_ADMIN_FIRST_LOGIN, &mut buf).await {
            Ok(1) => buf[0] == b'1',
            _ => false,
        }
    };
    Json(StatusResponse {
        version: "1.0.0",
        first_login,
    })
}
