//! ekv-based persistent storage for PDU configuration.
//!
//! Flash layout: CONFIG_START (0x10087000), 64 × 4KB pages.
//! ekv requires lexicographically-ascending key order within a write transaction.
//!
//! Note: ekv depends on embassy-sync 0.6.x, which is a different crate version
//! from the embassy-sync 0.7.x used elsewhere. We alias embassy-sync 0.6.x as
//! `embassy_sync_ekv` in Cargo.toml and use its RawMutex for the Database type.

use embassy_rp::flash::{Async as FlashAsync, Flash};
use embassy_sync_ekv::blocking_mutex::raw::CriticalSectionRawMutex;
use ekv::flash::PageID;
use ekv::{Config, Database};
use embedded_storage_async::nor_flash::NorFlash;
use static_cell::StaticCell;

use crate::config::{CONFIG_START, EKV_PAGE_COUNT, EKV_PAGE_SIZE, FLASH_SIZE};

// Flash base address on RP2040 — embassy-rp flash offsets are relative to this.
const FLASH_BASE: u32 = 0x10000000;
// Offset of CONFIG_START from the beginning of flash.
const CONFIG_OFFSET: u32 = CONFIG_START - FLASH_BASE;

// ── Flash adapter ──────────────────────────────────────────────────────────────

pub struct PduFlash<'d> {
    flash: Flash<'d, embassy_rp::peripherals::FLASH, FlashAsync, FLASH_SIZE>,
}

impl<'d> PduFlash<'d> {
    pub fn new(flash: Flash<'d, embassy_rp::peripherals::FLASH, FlashAsync, FLASH_SIZE>) -> Self {
        Self { flash }
    }

    /// Convert an ekv page ID to a flash offset (from flash start, not absolute).
    fn page_offset(page_id: PageID) -> u32 {
        CONFIG_OFFSET + (page_id.index() as u32) * (EKV_PAGE_SIZE as u32)
    }
}

impl<'d> ekv::flash::Flash for PduFlash<'d> {
    type Error = embassy_rp::flash::Error;

    fn page_count(&self) -> usize {
        EKV_PAGE_COUNT
    }

    async fn erase(&mut self, page_id: PageID) -> Result<(), Self::Error> {
        let offset = Self::page_offset(page_id);
        NorFlash::erase(&mut self.flash, offset, offset + EKV_PAGE_SIZE as u32).await
    }

    async fn read(
        &mut self,
        page_id: PageID,
        offset: usize,
        data: &mut [u8],
    ) -> Result<(), Self::Error> {
        let flash_offset = Self::page_offset(page_id) + offset as u32;
        embedded_storage_async::nor_flash::ReadNorFlash::read(
            &mut self.flash,
            flash_offset,
            data,
        )
        .await
    }

    async fn write(
        &mut self,
        page_id: PageID,
        offset: usize,
        data: &[u8],
    ) -> Result<(), Self::Error> {
        let flash_offset = Self::page_offset(page_id) + offset as u32;
        NorFlash::write(&mut self.flash, flash_offset, data).await
    }
}

// ── Database type ──────────────────────────────────────────────────────────────

/// ekv database backed by flash.
///
/// Uses `CriticalSectionRawMutex` from `embassy-sync 0.6.x` (aliased as
/// `embassy_sync_ekv`) to satisfy ekv's `RawMutex` bound.
pub type PduDatabase = Database<PduFlash<'static>, CriticalSectionRawMutex>;

static DATABASE: StaticCell<PduDatabase> = StaticCell::new();

/// Create and mount (or format + seed) the ekv database.
/// Returns `&'static PduDatabase`.
///
/// # Safety
/// Must only be called once (enforced by `StaticCell`).
pub async fn init_database(
    flash: Flash<'static, embassy_rp::peripherals::FLASH, FlashAsync, FLASH_SIZE>,
    random_seed: u32,
) -> &'static PduDatabase {
    let pdu_flash = PduFlash::new(flash);
    // Config is #[non_exhaustive]; use Default::default() and set fields separately.
    let mut config = Config::default();
    config.random_seed = random_seed;
    let db = DATABASE.init(Database::new(pdu_flash, config));

    // Try to mount; format + seed if mount fails (first boot / corrupted flash).
    match db.mount().await {
        Ok(()) => {}
        Err(_) => {
            db.format().await.unwrap();
            seed_defaults(db).await;
        }
    }
    db
}

// ── Schema key helpers ─────────────────────────────────────────────────────────

/// ekv key constants (sorted: "admin/..." < "init" < "p/..." < "u/...")

pub const KEY_INIT: &[u8] = b"init";
pub const KEY_ADMIN_FIRST_LOGIN: &[u8] = b"admin/first_login";

/// Build port name key: `b"p/{port}/name"` (port 0–7)
pub fn port_name_key(port: u8) -> heapless::Vec<u8, 16> {
    let mut v = heapless::Vec::new();
    v.extend_from_slice(b"p/").unwrap();
    v.push(b'0' + port).unwrap();
    v.extend_from_slice(b"/name").unwrap();
    v
}

/// Build user admin flag key: `b"u/{username}/admin"`
pub fn user_admin_key(username: &str) -> heapless::Vec<u8, 64> {
    let mut v = heapless::Vec::new();
    v.extend_from_slice(b"u/").unwrap();
    v.extend_from_slice(username.as_bytes()).unwrap();
    v.extend_from_slice(b"/admin").unwrap();
    v
}

/// Build user ports bitmask key: `b"u/{username}/ports"`
pub fn user_ports_key(username: &str) -> heapless::Vec<u8, 64> {
    let mut v = heapless::Vec::new();
    v.extend_from_slice(b"u/").unwrap();
    v.extend_from_slice(username.as_bytes()).unwrap();
    v.extend_from_slice(b"/ports").unwrap();
    v
}

/// Build user password hash key: `b"u/{username}/pw"`
pub fn user_pw_key(username: &str) -> heapless::Vec<u8, 64> {
    let mut v = heapless::Vec::new();
    v.extend_from_slice(b"u/").unwrap();
    v.extend_from_slice(username.as_bytes()).unwrap();
    v.extend_from_slice(b"/pw").unwrap();
    v
}

// ── Defaults seeding ───────────────────────────────────────────────────────────

/// Write initial defaults. Keys MUST be in lexicographic order within each transaction.
///
/// Sorted key order across all transactions:
/// - `b"admin/first_login"`
/// - `b"init"`
/// - `b"p/0/name"` .. `b"p/7/name"`
/// - `b"u/admin/admin"` < `b"u/admin/ports"` < `b"u/admin/pw"`
pub async fn seed_defaults(db: &PduDatabase) {
    // Transaction 1: keys "admin/first_login" then "init" (a < i)
    {
        let mut wtx = db.write_transaction().await;
        wtx.write(b"admin/first_login", b"1").await.unwrap();
        wtx.write(b"init", b"1").await.unwrap();
        wtx.commit().await.unwrap();
    }

    // Transaction 2: port names p/0/name .. p/7/name (already sorted lexicographically)
    {
        let mut wtx = db.write_transaction().await;
        for i in 0..8u8 {
            let key = port_name_key(i);
            let mut name = heapless::Vec::<u8, 16>::new();
            name.extend_from_slice(b"Port ").unwrap();
            name.push(b'0' + i).unwrap();
            wtx.write(&key, &name).await.unwrap();
        }
        wtx.commit().await.unwrap();
    }

    // Transaction 3: admin user
    // Sorted: u/admin/admin < u/admin/ports < u/admin/pw
    //         'admin' ('a'=97) < 'ports' ('p'=112) < 'pw' ('p','w': "po.." < "pw")
    {
        let mut wtx = db.write_transaction().await;
        let admin_key = user_admin_key("admin");
        let ports_key = user_ports_key("admin");
        let pw_key = user_pw_key("admin");
        wtx.write(&admin_key, b"1").await.unwrap();
        wtx.write(&ports_key, &[0xFFu8]).await.unwrap(); // all 8 ports enabled
        wtx.write(&pw_key, &hash_password("admin")).await.unwrap();
        wtx.commit().await.unwrap();
    }
}

// ── Password hashing ───────────────────────────────────────────────────────────

/// SHA-256 hash of a password string.
pub fn hash_password(password: &str) -> [u8; 32] {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(password.as_bytes());
    hasher.finalize().into()
}
