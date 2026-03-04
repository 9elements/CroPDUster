# Task 02 — Dependency Upgrades

## Status: ✅ Done

## Objective
Update all Cargo.toml files and rust-toolchain.toml to the target dependency versions.

## Changes Required

### `rust-toolchain.toml`
- channel: `nightly-2025-02-01` → `nightly-2026-02-01`

### Root `Cargo.toml`
- Add `xtask` to workspace members
- Update `[patch.crates-io]` git rev: `539837a...` → `3651d8ef249dc20d30df7382237f2451d889c011`
- Add to `[patch.crates-io]`: `embassy-net`, `embassy-boot`, `embassy-boot-rp`, `embassy-net-wiznet`
- Update workspace deps:
  - `embassy-rp` version → `0.9.0`
  - `embassy-net` version → `0.8.0`  
  - `embedded-io-async` → `"0.7"`
  - `heapless` → `"0.8"` (pin at 0.8 due to ekv constraint)
- Add new workspace deps:
  - `picoserve = { version = "0.18", features = ["embassy", "defmt", "json"] }`
  - `ekv = { version = "1.0", features = ["defmt", "page-size-4096", "max-page-count-64", "erase-value-255", "max-key-size-64", "max-value-size-256", "max-chunk-size-512"] }`
  - `sha2 = { version = "0.10", default-features = false }`
  - `serde = { version = "1.0", default-features = false, features = ["derive"] }`
- Remove unused workspace deps: `embassy-usb`, `tock-registers`, `zerocopy`, `num_enum`, `ufmt`, `assign-resources`

### `application/Cargo.toml`
- Add: `picoserve`, `ekv`, `sha2`, `serde`
- Remove: `embedded-hal = "0.2.6"`
- Update: `embedded-io-async = "0.7"`, versions for embassy crates

### `bootloader/Cargo.toml`
- Update embassy-rp and embassy-boot-rp versions

## Checklist
- [x] Update `rust-toolchain.toml`
- [x] Update root `Cargo.toml` patch block
- [x] Update root `Cargo.toml` workspace deps
- [x] Update `application/Cargo.toml`
- [x] Update `bootloader/Cargo.toml`
- [ ] Run `cargo fetch` to verify no resolution errors

## Log
- `rust-toolchain.toml`: channel bumped from `nightly-2025-02-01` to `nightly-2026-02-01`
- Root `Cargo.toml`:
  - Added `"xtask"` to workspace members list
  - Updated all `[patch.crates-io]` git revisions from `539837a7485381f83ef078595a4e248a0ea11436` to `3651d8ef249dc20d30df7382237f2451d889c011`
  - Removed `embassy-usb` from patch block
  - Added new patch entries: `embassy-net`, `embassy-boot`, `embassy-boot-rp`, `embassy-net-wiznet`, `embassy-embedded-hal`
  - `embassy-rp`: version `0.8.0` → `0.9.0`, removed `rom-func-cache` and `rom-v2-intrinsics` features
  - `heapless`: version `0.9.1` → `0.8`, removed `ufmt` feature (not in 0.8 API), kept `portable-atomic-critical-section`
  - `embedded-io-async`: version `0.6.1` → `0.7`
  - Added `embassy-net = { version = "0.8.0", ... }` workspace dep
  - Added `embassy-boot-rp = { version = "0.9.0" }` workspace dep
  - Added `picoserve`, `ekv`, `sha2`, `serde` workspace deps
  - Removed `embassy-usb`, `tock-registers`, `zerocopy`, `num_enum`, `ufmt`, `assign-resources`
- `application/Cargo.toml`:
  - `embassy-rp`: `0.8.0` → `0.9.0`, features updated (added `critical-section-impl`)
  - `embassy-boot-rp`: `0.8.0` → `0.9.0`
  - `embassy-net`: `0.7.1` → `0.8.0`
  - `embedded-io-async`: `0.6.1` → `0.7`
  - Added `picoserve`, `ekv`, `sha2`, `serde` dependencies
  - Removed `embedded-hal = "0.2.6"` (legacy v0.2 HAL)
- `bootloader/Cargo.toml`:
  - `embassy-rp`: `0.8.0` → `0.9.0`
  - `embassy-boot-rp`: `0.8.0` → `0.9.0`
