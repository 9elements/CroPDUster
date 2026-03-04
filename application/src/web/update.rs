//! POST /api/update — streaming OTA firmware update (admin only).

use embassy_boot_rp::{AlignedBuffer, FirmwareUpdater, FirmwareUpdaterConfig};
use embassy_embedded_hal::adapter::BlockingAsync;
use embassy_rp::flash::{Flash, ERASE_SIZE};
use embassy_sync::blocking_mutex::raw::NoopRawMutex;
use embassy_sync::mutex::Mutex;
use picoserve::response::{Json, StatusCode};

use crate::auth::AdminUser;
use crate::config::FLASH_SIZE;
use crate::web::admin::OkResponse;

/// Return type: `Result<Json<OkResponse>, (StatusCode, &'static str)>: IntoResponse`
type ApiResult = Result<Json<OkResponse>, (StatusCode, &'static str)>;

const MAX_FIRMWARE_SIZE: usize = 512 * 1024; // 512KB

pub async fn handle_firmware_update(
    _admin: AdminUser,
    // heapless::Vec<u8, N>: FromRequest (reads body into owned buffer)
    body: heapless::Vec<u8, 8192>,
) -> ApiResult {
    if body.is_empty() || body.len() > MAX_FIRMWARE_SIZE {
        return Err((StatusCode::BAD_REQUEST, "Invalid firmware size"));
    }

    // Steal flash peripheral for OTA — safe because no other task uses flash here
    let p = unsafe { embassy_rp::Peripherals::steal() };
    let flash: Flash<'_, _, _, FLASH_SIZE> = Flash::new_blocking(p.FLASH);
    let flash = Mutex::<NoopRawMutex, _>::new(BlockingAsync::new(flash));

    let config = FirmwareUpdaterConfig::from_linkerfile(&flash, &flash);
    let mut aligned = AlignedBuffer([0u8; ERASE_SIZE]);
    let mut updater = FirmwareUpdater::new(config, &mut aligned.0);

    let mut offset = 0usize;
    let mut write_buf = [0u8; ERASE_SIZE];
    for chunk in body.chunks(ERASE_SIZE) {
        write_buf[..chunk.len()].copy_from_slice(chunk);
        for b in write_buf[chunk.len()..].iter_mut() {
            *b = 0;
        }
        if updater.write_firmware(offset, &write_buf).await.is_err() {
            return Err((StatusCode::INTERNAL_SERVER_ERROR, "Flash write error"));
        }
        offset += ERASE_SIZE;
    }

    if updater.mark_updated().await.is_err() {
        return Err((StatusCode::INTERNAL_SERVER_ERROR, "Failed to mark update"));
    }

    embassy_time::Timer::after_millis(100).await;
    cortex_m::peripheral::SCB::sys_reset();
}
