//! Web layer: AppState, router, and web_task.

pub mod admin;
pub mod gpio;
pub mod sensors;
pub mod status;
pub mod update;

use picoserve::routing::{delete, get, parse_path_segment, post};
use picoserve::{AppWithStateBuilder, Router};

use crate::storage::PduDatabase;

// ── AppState ───────────────────────────────────────────────────────────────────

/// Module-level static DB pointer set once at startup.
/// All web handlers that need DB access call `crate::web::db()` directly.
/// This avoids picoserve HRTB / State-type issues: the router's state is `()`
/// so `picoserve::Server::new` accepts it directly.
///
/// Stored as `usize` (pointer-as-integer) in a `portable_atomic::AtomicUsize`
/// so the static is `Sync` without needing unsafe.
static DB_PTR: portable_atomic::AtomicUsize =
    portable_atomic::AtomicUsize::new(0);

/// Initialise the module-level DB handle. Call exactly once before spawning web tasks.
pub fn init_db(db: &'static PduDatabase) {
    DB_PTR.store(
        db as *const PduDatabase as usize,
        portable_atomic::Ordering::Relaxed,
    );
}

/// Borrow the module-level database. Panics if `init_db` was not called.
pub fn db() -> &'static PduDatabase {
    let ptr = DB_PTR.load(portable_atomic::Ordering::Relaxed) as *const PduDatabase;
    assert!(!ptr.is_null(), "web::init_db() not called");
    // Safety: pointer is valid for the lifetime of the program (points to a 'static value).
    unsafe { &*ptr }
}

// ── Config ─────────────────────────────────────────────────────────────────────

/// Number of concurrent web task instances (each handles one TCP connection).
pub const WEB_TASK_POOL_SIZE: usize = 4;

/// picoserve server configuration.
pub static CONFIG: picoserve::Config = picoserve::Config::new(picoserve::Timeouts {
    start_read_request: picoserve::time::Duration::from_secs(5),
    persistent_start_read_request: picoserve::time::Duration::from_secs(1),
    read_request: picoserve::time::Duration::from_secs(10),
    write: picoserve::time::Duration::from_secs(10),
});

// ── Router builder ─────────────────────────────────────────────────────────────

/// Builder that constructs the full router.
///
/// State is `()` because auth extractors and handlers all access the DB via
/// the module-level `db()` static.  A stateless router is required by
/// `picoserve::Server::new`.
pub struct App;

impl AppWithStateBuilder for App {
    type State = ();
    type PathRouter = impl picoserve::routing::PathRouter<()>;

    fn build_app(self) -> Router<Self::PathRouter, ()> {
        Router::new()
            // Static index
            .route("/", get(serve_index))
            // Status
            .route("/api/status", get(status::handle_status))
            // Sensors
            .route("/api/sensors", get(sensors::handle_sensors))
            // GPIO — pin parameter parsed from path
            .route(
                ("/api/gpio/", parse_path_segment::<u8>()),
                get(gpio::handle_gpio_get),
            )
            .route(
                ("/api/gpio/", parse_path_segment::<u8>(), "/toggle"),
                post(gpio::handle_gpio_toggle),
            )
            .route(
                ("/api/gpio/", parse_path_segment::<u8>(), "/set"),
                post(gpio::handle_gpio_set),
            )
            // Port names
            .route(
                ("/api/port/", parse_path_segment::<u8>(), "/name"),
                get(admin::handle_port_name_get).post(admin::handle_port_name_set),
            )
            // Admin endpoints
            .route("/api/admin/password", post(admin::handle_password_change))
            .route("/api/admin/users", post(admin::handle_create_user))
            .route(
                (
                    "/api/admin/users/",
                    parse_path_segment::<heapless::String<32>>(),
                    "/ports",
                ),
                post(admin::handle_set_user_ports),
            )
            .route(
                (
                    "/api/admin/users/",
                    parse_path_segment::<heapless::String<32>>(),
                ),
                delete(admin::handle_delete_user),
            )
            .route("/api/admin/reset", post(admin::handle_factory_reset))
            // OTA firmware update
            .route("/api/update", post(update::handle_firmware_update))
    }
}

// ── Index handler ──────────────────────────────────────────────────────────────

const INDEX_HTML: &str = include_str!("../index.html");

async fn serve_index() -> impl picoserve::response::IntoResponse {
    (("Content-Type", "text/html; charset=utf-8"), INDEX_HTML)
}

// ── Web task ───────────────────────────────────────────────────────────────────

#[embassy_executor::task(pool_size = WEB_TASK_POOL_SIZE)]
pub async fn web_task(
    id: usize,
    stack: embassy_net::Stack<'static>,
    app: &'static picoserve::AppRouter<App>,
    config: &'static picoserve::Config,
) -> ! {
    let port = 80;
    let mut rx_buffer = [0u8; 4096];
    let mut tx_buffer = [0u8; 4096];
    let mut http_buffer = [0u8; 2048];

    loop {
        picoserve::Server::new(app, config, &mut http_buffer)
            .listen_and_serve(id, stack, port, &mut rx_buffer, &mut tx_buffer)
            .await;
    }
}
