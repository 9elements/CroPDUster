# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

This is a Rust embedded project for the Raspberry Pi Pico (RP2040) that implements a bootloader-based firmware update system. The project consists of two main components:

1. **Bootloader** (`bootloader/`): Uses `embassy-boot-rp` to manage firmware updates with flash partitioning
2. **Application** (`application/`): Contains example applications demonstrating OTA firmware updates

The bootloader enables safe over-the-air firmware updates by maintaining separate ACTIVE and DFU (Device Firmware Update) partitions.

## Build Commands

### Bootloader

Flash the bootloader (required before any application):
```bash
cd bootloader
cargo flash --release --chip RP2040
```

Run with debugging enabled:
```bash
cd bootloader
cargo run --release --features debug
```

### Application

Build application binary 'b' (the update target):
```bash
cd application
cargo build --release --bin b
cargo objcopy --release --bin b -- -O binary b.bin
```

Flash application 'a' (which updates to 'b'):
```bash
cd application
cargo flash --release --bin a --chip RP2040
```

Build main application (Ethernet-enabled):
```bash
cd application
cargo build --release
```

## Prerequisites

- Rust nightly toolchain (2025-02-01)
- `thumbv6m-none-eabi` target
- `cargo-binutils` (for objcopy)
- `cargo-flash` (for flashing to device)
- `flip-link` (linker for stack overflow protection)
- `elf2uf2-rs` (UF2 converter)
- Optional: `probe-rs` for debugging

## Architecture

### Flash Memory Layout

The flash is partitioned as follows (see `memory.x` files):

- **BOOT2**: `0x10000000` - `0x100` (256 bytes) - RP2040 second-stage bootloader
- **Bootloader Flash**: `0x10000100` - 24KB - Bootloader code
- **BOOTLOADER_STATE**: `0x10006000` - 4KB - Bootloader state/metadata
- **ACTIVE**: `0x10007000` - 512KB - Currently running application
- **DFU**: `0x10087000` - 516KB - Staged firmware update

The bootloader reads from BOOTLOADER_STATE to determine which partition to boot and manages copying DFU to ACTIVE on successful updates.

### Bootloader Operation

The bootloader (`bootloader/src/main.rs`) performs these operations on boot:

1. Initializes flash with watchdog timer (8 second timeout)
2. Reads configuration from linker-defined memory regions
3. Checks BOOTLOADER_STATE for pending updates
4. If update marked, copies DFU partition to ACTIVE
5. Jumps to ACTIVE partition to execute application

Hard faults trigger system reset to retry boot.

### Application Binary Structure

The example applications demonstrate the update workflow:

- **Binary 'a'** (`src/a.rs`): Initial application that waits 5 seconds, then writes binary 'b' to the DFU partition and marks it for update. Triggers reset to activate bootloader.
- **Binary 'b'** (`src/b.rs`): Simple LED blink application. Does NOT mark itself as active, so on reset the bootloader rolls back to 'a', which re-updates to 'b' (demonstration loop).

Binary 'b' is embedded into binary 'a' via `include_bytes!("../../b.bin")`.

### Main Application

The primary application (`src/main.rs`) implements a TCP echo server using:

- **embassy-net-wiznet**: Ethernet driver for W5500 chip
- **embassy-net**: Network stack with DHCP support
- **embassy-executor**: Async executor for task scheduling
- Pin configuration for W5500-EVB-Pico board (SPI on GPIO 16-21, LED on GPIO 25)

The application spawns two tasks: `ethernet_task` for W5500 management and `net_task` for network stack operations.

## Cargo Workspace

This is a Cargo workspace with two members:

- `bootloader`: Bootloader binary
- `application`: Application binaries

Dependencies use a patched Embassy framework from a specific git revision (`539837a748...`). Local paths point to a sibling `embassy-*` repository structure, indicating this is part of a larger development environment.

## Development Workflow

1. Flash bootloader first (only needs to be done once)
2. Build application binary 'b' and convert to `.bin` format
3. Flash application binary 'a' (which includes 'b')
4. Device boots 'a', which updates to 'b' after 5 seconds
5. On reset, bootloader may roll back to 'a' (if 'b' doesn't mark itself active)

For production firmware: applications should call `updater.mark_booted()` after successful boot to prevent rollback.

## Toolchain Configuration

- Nightly Rust channel: `nightly-2025-02-01`
- Target: `thumbv6m-none-eabi` (Cortex-M0+ architecture)
- Required components: `rust-src`, `rustfmt`, `llvm-tools`, `miri`
- Release profile: optimized for size (`opt-level = "s"`), LTO enabled

## Debugging

The `.cargo/config.toml` files configure:

- Linker: `flip-link` (stack overflow protection)
- Runner: `elf2uf2-rs` (default) or `probe-rs` (commented out)
- `DEFMT_LOG=debug` environment variable for logging

To use probe-rs instead of UF2 flashing, uncomment the probe-rs runner line in `.cargo/config.toml`.

The bootloader includes a commented delay loop that can be uncommented when debugging with RTT to prevent flash access faults during early boot.
