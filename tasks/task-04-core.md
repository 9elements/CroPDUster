# Task 04 — Application Core Modules

## Status: ⏳ Pending

## Objective
Create all non-web application modules: config constants, ekv storage layer,
HTTP auth extractors, GPIO task, and sensor task.

## Files to Create

### `application/src/config.rs`
Constants: `FLASH_SIZE`, `DFU_START`, `DFU_SIZE`, `CONFIG_START`, `CONFIG_SIZE`,
`EKV_PAGE_SIZE`, `EKV_PAGE_COUNT`, `PORT_COUNT`, `MAX_USERS`, pin assignments.

### `application/src/storage.rs`
- `PduFlash<'d>` struct implementing `ekv::flash::Flash`
- `PduDatabase` type alias: `ekv::Database<PduFlash<'static>, CriticalSectionRawMutex>`
- `DATABASE` static: `StaticCell<PduDatabase>`
- Schema key constants (all as `&[u8]`)
- Key builder functions: `user_pw_key`, `user_admin_key`, `user_ports_key`, `port_name_key`
- `init_database(db)` → mounts or formats + seeds defaults
- `seed_defaults(db)` → writes all initial keys in sorted order

### `application/src/auth.rs`
- `hash_password(password: &str) -> [u8; 32]` — SHA-256
- `AuthUser { username, is_admin, allowed_ports }` struct
- `AuthError` enum (Missing, InvalidEncoding, InvalidCredentials, StorageError) → 401
- `impl FromRequestParts<AppState> for AuthUser`
- `AdminUser(AuthUser)` wrapper → 403 if not admin
- `impl FromRequestParts<AppState> for AdminUser`

### `application/src/gpio.rs`
- `GpioCommand` enum: `Toggle(u8)`, `Set(u8, bool)`
- `GPIO_SIGNAL: Signal<CriticalSectionRawMutex, GpioCommand>` static
- `GPIO_STATES: StaticCell<Mutex<CriticalSectionRawMutex, [bool; 8]>>`
- `gpio_task(gpio0..gpio7, states)` — 8 output pins, updates states mutex

### `application/src/sensors.rs`
- `SensorData { temperature_c: f32, voltage_v: f32, current_a: [f32; 8] }` struct
- `SENSOR_DATA: StaticCell<Mutex<CriticalSectionRawMutex, SensorData>>`
- `sensor_task(adc, ts_channel, data)` — reads RP2040 internal ADC, updates every 5s

## Key Implementation Details

### ekv Flash Adapter
```rust
const CONFIG_START: u32 = 0x10087000;
// Page IDs map to: CONFIG_START + page_id * 4096

impl ekv::flash::Flash for PduFlash<'_> {
    fn page_count(&self) -> usize { 64 }
    async fn erase(&mut self, page_id: PageID) { ... }
    async fn read(&mut self, page_id: PageID, offset: usize, data: &mut [u8]) { ... }
    async fn write(&mut self, page_id: PageID, offset: usize, data: &[u8]) { ... }
}
```

### Seed key order (must be lexicographically sorted)
```
b"admin/first_login"
b"init"
b"p/0/name" .. b"p/7/name"
b"u/admin/admin"
b"u/admin/ports"
b"u/admin/pw"
```

### Auth extractor flow
1. Extract `Authorization: Basic <b64>` header
2. base64-decode → `username:password`
3. SHA-256 hash the password
4. ekv read transaction: lookup `u/{username}/pw`
5. Compare hashes
6. Read admin flag and port bitmask
7. Return `AuthUser`

### RP2040 temperature formula
```rust
let voltage = (raw as f32) * 3.3 / 4096.0;
let temp_c = 27.0 - (voltage - 0.706) / 0.001721;
```

## Checklist
- [ ] Create `application/src/config.rs`
- [ ] Create `application/src/storage.rs`
- [ ] Create `application/src/auth.rs`
- [ ] Create `application/src/gpio.rs`
- [ ] Create `application/src/sensors.rs`

## Log
<!-- Agent fills this in -->
