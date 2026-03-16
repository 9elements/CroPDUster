//! HTTP Basic Auth extractors for picoserve.
//!
//! Implements `FromRequestParts<'r, ()>` for:
//! - `AuthUser` — any authenticated user
//! - `AdminUser` — admin-only wrapper (403 if not admin)
//!
//! DB access is via the module-level `crate::web::db()` static so that no
//! state type is needed (picoserve `Server::new` requires `State = ()`).

use base64::Engine as _;
use picoserve::extract::FromRequestParts;
use picoserve::request::RequestParts;
use picoserve::response::{Connection, IntoResponse, ResponseWriter, StatusCode};
use picoserve::ResponseSent;

use crate::storage::{user_admin_key, user_ports_key, user_pw_key};

// Re-export hash_password so other modules can use it from auth.
pub use crate::storage::hash_password;

// ── AuthUser ──────────────────────────────────────────────────────────────────

/// An authenticated PDU user, extracted from HTTP Basic Auth credentials.
#[derive(Debug, Clone)]
pub struct AuthUser {
    pub username: heapless::String<32>,
    pub is_admin: bool,
    /// Bitmask of permitted relay ports (bit N = port N allowed).
    pub allowed_ports: u8,
}

// ── AuthError ─────────────────────────────────────────────────────────────────

/// Reasons why authentication can fail.
pub enum AuthError {
    /// No `Authorization` header present.
    Missing,
    /// Header value is not valid Base64 or UTF-8.
    InvalidEncoding,
    /// Username/password pair does not match stored credentials.
    InvalidCredentials,
    /// Flash storage returned an unexpected error.
    StorageError,
}

impl IntoResponse for AuthError {
    async fn write_to<R: embedded_io_async::Read, W: ResponseWriter<Error = R::Error>>(
        self,
        connection: Connection<'_, R>,
        response_writer: W,
    ) -> Result<ResponseSent, W::Error> {
        let status = match self {
            AuthError::Missing | AuthError::InvalidEncoding | AuthError::InvalidCredentials => {
                StatusCode::UNAUTHORIZED
            }
            AuthError::StorageError => StatusCode::INTERNAL_SERVER_ERROR,
        };
        (status, "Unauthorized")
            .write_to(connection, response_writer)
            .await
    }
}

impl<'r> FromRequestParts<'r, ()> for AuthUser {
    type Rejection = AuthError;

    async fn from_request_parts(
        _state: &'r (),
        request_parts: &RequestParts<'r>,
    ) -> Result<Self, Self::Rejection> {
        // 1. Find Authorization header (case-insensitive)
        let auth_header = request_parts
            .headers()
            .get("Authorization")
            .ok_or(AuthError::Missing)?;

        let raw = auth_header.as_raw();

        // 2. Strip "Basic " prefix
        const BASIC_PREFIX: &[u8] = b"Basic ";
        if raw.len() < BASIC_PREFIX.len()
            || !raw[..BASIC_PREFIX.len()].eq_ignore_ascii_case(BASIC_PREFIX)
        {
            return Err(AuthError::InvalidEncoding);
        }
        let b64 = &raw[BASIC_PREFIX.len()..];

        // 3. Base64-decode into a stack buffer (username:password, max ~96 bytes)
        let mut decoded = [0u8; 128];
        let n = base64::engine::general_purpose::STANDARD
            .decode_slice(b64, &mut decoded)
            .map_err(|_| AuthError::InvalidEncoding)?;

        let decoded_str =
            core::str::from_utf8(&decoded[..n]).map_err(|_| AuthError::InvalidEncoding)?;

        // 4. Split "username:password"
        let colon = decoded_str.find(':').ok_or(AuthError::InvalidEncoding)?;
        let username = &decoded_str[..colon];
        let password = &decoded_str[colon + 1..];

        // 5. Hash the submitted password
        let pw_hash = hash_password(password);

        // 6. Read stored hash, admin flag, and port mask from ekv via module-level static
        let db = crate::web::db();
        let pw_key = user_pw_key(username);
        let admin_key = user_admin_key(username);
        let ports_key = user_ports_key(username);

        let mut stored_pw = [0u8; 32];
        let mut is_admin_bytes = [0u8; 1];
        let mut ports_bytes = [0u8; 1];

        {
            let rtx = db.read_transaction().await;

            let n = rtx
                .read(&pw_key, &mut stored_pw)
                .await
                .map_err(|_| AuthError::InvalidCredentials)?;
            if n != 32 {
                return Err(AuthError::InvalidCredentials);
            }

            rtx.read(&admin_key, &mut is_admin_bytes)
                .await
                .map_err(|_| AuthError::StorageError)?;

            rtx.read(&ports_key, &mut ports_bytes)
                .await
                .map_err(|_| AuthError::StorageError)?;
        }

        // 7. Constant-time compare (best-effort on Cortex-M0+)
        if stored_pw != pw_hash {
            return Err(AuthError::InvalidCredentials);
        }

        let mut uname: heapless::String<32> = heapless::String::new();
        uname
            .push_str(username)
            .map_err(|_| AuthError::InvalidEncoding)?;

        Ok(AuthUser {
            username: uname,
            is_admin: is_admin_bytes[0] == b'1',
            allowed_ports: ports_bytes[0],
        })
    }
}

// ── AdminUser ─────────────────────────────────────────────────────────────────

/// Wrapper extractor that requires the authenticated user to be an admin.
/// The inner `AuthUser` is available for handlers that need it via `.0`.
#[allow(dead_code)]
pub struct AdminUser(pub AuthUser);

/// Returned when an authenticated-but-non-admin user tries to access an admin route.
pub struct NotAdmin;

impl IntoResponse for NotAdmin {
    async fn write_to<R: embedded_io_async::Read, W: ResponseWriter<Error = R::Error>>(
        self,
        connection: Connection<'_, R>,
        response_writer: W,
    ) -> Result<ResponseSent, W::Error> {
        (StatusCode::FORBIDDEN, "Forbidden")
            .write_to(connection, response_writer)
            .await
    }
}

impl<'r> FromRequestParts<'r, ()> for AdminUser {
    type Rejection = NotAdmin;

    async fn from_request_parts(
        state: &'r (),
        request_parts: &RequestParts<'r>,
    ) -> Result<Self, Self::Rejection> {
        let user = AuthUser::from_request_parts(state, request_parts)
            .await
            .map_err(|_| NotAdmin)?;
        if user.is_admin {
            Ok(AdminUser(user))
        } else {
            Err(NotAdmin)
        }
    }
}
