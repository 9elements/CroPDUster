# Task 03 — xtask Build System

## Status: ⏳ Pending

## Objective
Replace the Makefile with a `cargo xtask` binary. Delete the Makefile.
Create `.cargo/config.toml` at the workspace root with the xtask alias.

## Files to Create
- `xtask/Cargo.toml`
- `xtask/src/main.rs`
- `.cargo/config.toml` (workspace root)

## Files to Delete
- `Makefile`

## Subcommands

### `cargo xtask build [--bootloader] [--application]`
- Default (no flags): build both
- Runs `cargo build --release` in the appropriate crate directory
- Copies ELF to `build/` and converts to UF2 via `elf2uf2-rs`

### `cargo xtask combine`
- Invokes `python3 scripts/combine_binaries.py` with correct arguments
- Requires both ELFs to already be in `build/`

### `cargo xtask dist`
- Calls build (both) then combine

### `cargo xtask flash [--bootloader] [--application] [--probe] [--ota <ip>]`
- Default (no flags): flash combined UF2 via BOOTSEL (UF2 drag-and-drop)
- `--probe`: use `probe-rs run --chip RP2040`
- `--ota <ip>`: HTTP POST to `http://<ip>/api/update` using ureq
  - Reads credentials from `.pdu-credentials` file (format: `user:pass`) or prompts

### `cargo xtask check-tools`
- Checks: `elf2uf2-rs`, `flip-link`, `python3`, optionally `probe-rs`
- Prints install instructions for missing tools

### `cargo xtask clean`
- Runs `cargo clean` in bootloader/ and application/
- Deletes `build/` directory

## xtask Dependencies
```toml
anyhow = "1"
xshell = "0.2"
clap = { version = "4", features = ["derive"] }
ureq = "2"
base64 = "0.22"
```

## UF2 Flash Logic (Linux)
- Watch for `/dev/disk/by-label/RPI-RP2` to appear (poll with timeout)
- Mount it to a temp dir
- Copy UF2 file
- Unmount

## Checklist
- [ ] Create `xtask/Cargo.toml`
- [ ] Create `xtask/src/main.rs` with all subcommands
- [ ] Create `.cargo/config.toml` with xtask alias
- [ ] Delete `Makefile`
- [ ] Test: `cargo xtask --help` works from workspace root
- [ ] Test: `cargo xtask check-tools` runs without panic

## Log
<!-- Agent fills this in -->
