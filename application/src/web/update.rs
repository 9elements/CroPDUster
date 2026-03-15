//! POST /api/update — streaming OTA firmware update (admin only).
//!
//! Uses a `MethodHandlerService` instead of a plain handler function so the
//! request body is streamed to flash in ERASE_SIZE chunks rather than being
//! buffered in a large `heapless::Vec`.  This keeps the async state machine
//! small and avoids the ~450 KB static RAM cost of the previous approach.

use embassy_boot_rp::{AlignedBuffer, FirmwareUpdater, FirmwareUpdaterConfig};
use embassy_embedded_hal::adapter::BlockingAsync;
use embassy_rp::flash::{Flash, ERASE_SIZE};
use embassy_sync::blocking_mutex::raw::NoopRawMutex;
use embassy_sync::mutex::Mutex;
use picoserve::extract::FromRequestParts;
use picoserve::io::Read;
use picoserve::request::Request;
use picoserve::response::{IntoResponse, ResponseWriter, StatusCode};
use picoserve::routing::MethodHandlerService;
use picoserve::ResponseSent;

use crate::auth::AdminUser;
use crate::config::{ACTIVE_SIZE, FLASH_SIZE};

// ── Service struct ─────────────────────────────────────────────────────────────

/// Handles `POST /api/update`.
pub struct FirmwareUpdateService;

impl MethodHandlerService<(), ()> for FirmwareUpdateService {
    async fn call_method_handler_service<R: Read, W: ResponseWriter<Error = R::Error>>(
        &self,
        state: &(),
        _path_parameters: (),
        method: &str,
        request: Request<'_, R>,
        response_writer: W,
    ) -> Result<ResponseSent, W::Error> {
        let Request {
            parts,
            mut body_connection,
        } = request;

        // Only POST is allowed.
        if method != "POST" {
            return (StatusCode::METHOD_NOT_ALLOWED, "Method Not Allowed")
                .write_to(body_connection.finalize().await?, response_writer)
                .await;
        }

        // Authenticate: require admin.
        match AdminUser::from_request_parts(state, &parts).await {
            Ok(_admin) => {}
            Err(rejection) => {
                return rejection
                    .write_to(body_connection.finalize().await?, response_writer)
                    .await;
            }
        }

        // Stream the body to flash in ERASE_SIZE chunks.
        let result = write_firmware_streaming(body_connection.body()).await;

        let conn = body_connection.finalize().await?;

        match result {
            Ok(()) => (StatusCode::OK, "{\"ok\":true}").write_to(conn, response_writer).await,
            Err(msg) => (StatusCode::INTERNAL_SERVER_ERROR, msg)
                .write_to(conn, response_writer)
                .await,
        }
    }
}

// ── Streaming flash writer ─────────────────────────────────────────────────────

async fn write_firmware_streaming<'r, R: Read>(
    body: picoserve::request::RequestBody<'r, R>,
) -> Result<(), &'static str> {
    let content_length = body.content_length();
    if content_length == 0 || content_length > ACTIVE_SIZE as usize {
        return Err("Invalid firmware size");
    }

    let p = unsafe { embassy_rp::Peripherals::steal() };
    let flash: Flash<'_, _, _, FLASH_SIZE> = Flash::new_blocking(p.FLASH);
    let flash = Mutex::<NoopRawMutex, _>::new(BlockingAsync::new(flash));

    let config = FirmwareUpdaterConfig::from_linkerfile(&flash, &flash);
    let mut aligned = AlignedBuffer([0u8; ERASE_SIZE]);
    let mut updater = FirmwareUpdater::new(config, &mut aligned.0);

    let mut write_buf = [0u8; ERASE_SIZE];
    let mut offset = 0usize;
    let mut remaining = content_length;

    let mut reader = body.reader();

    while remaining > 0 {
        let chunk_size = remaining.min(ERASE_SIZE);

        // Pad with 0xFF (NOR flash erased state) so we always write a full erase block.
        for b in write_buf[chunk_size..].iter_mut() {
            *b = 0xFF;
        }

        // Read exactly `chunk_size` bytes from the request stream.
        reader
            .read_exact(&mut write_buf[..chunk_size])
            .await
            .map_err(|_| "Read error")?;

        updater
            .write_firmware(offset, &write_buf)
            .await
            .map_err(|_| "Flash write error")?;

        offset += ERASE_SIZE;
        remaining -= chunk_size;
    }

    updater.mark_updated().await.map_err(|_| "Mark update error")?;

    embassy_time::Timer::after_millis(100).await;
    cortex_m::peripheral::SCB::sys_reset();
}
