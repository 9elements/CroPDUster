# CroPDUster Pico — Implementation Plan

## Project Goal

Implement a full network-attached PDU (Power Distribution Unit) controller on the
Raspberry Pi Pico (RP2040) with:

- **8 relay-controlled power outlets** (GPIO 0–7)
- **W5500 Ethernet** with DHCP (SPI0)
- **embassy-boot-rp** A/B OTA firmware update
- **picoserve** HTTP server replacing the hand-rolled HTTP parser
- **HTTP Basic Auth** — multi-user, admin + regular users
- **Per-user port ACL** (bitmask, up to 8 ports per user)
- **Port renaming** (stored persistently)
- **ekv** key-value store on a dedicated CONFIG flash region
- **Sensor display** — RP2040 internal temperature ADC + stubs for current/voltage
- **Factory reset** via GPIO 26 (held low at boot) or admin API/UI
- **xtask** build system replacing the Makefile
- **Single-page web UI** in vanilla JS/HTML

---

## Flash Memory Layout

```
Address      Size     Name              Notes
0x10000000   256 B    BOOT2             RP2040 second-stage bootloader
0x10000100   ~24 KB   Bootloader code   embassy-boot-rp
0x10006000     4 KB   BOOTLOADER_STATE  embassy-boot partition table
0x10007000   256 KB   ACTIVE            Running application (was 512 KB)
0x10047000   256 KB   DFU               Staged firmware update (was 516 KB)
0x10087000   256 KB   CONFIG            ekv key-value database (new)
0x200000000  264 KB   RAM
```

---

## Hardware Pin Assignment

| Pin     | Function          |
|---------|-------------------|
| GPIO 0  | Relay 0 output    |
| GPIO 1  | Relay 1 output    |
| GPIO 2  | Relay 2 output    |
| GPIO 3  | Relay 3 output    |
| GPIO 4  | Relay 4 output    |
| GPIO 5  | Relay 5 output    |
| GPIO 6  | Relay 6 output    |
| GPIO 7  | Relay 7 output    |
| GPIO 16 | W5500 MISO (SPI0) |
| GPIO 17 | W5500 CS          |
| GPIO 18 | W5500 CLK (SPI0)  |
| GPIO 19 | W5500 MOSI (SPI0) |
| GPIO 20 | W5500 RST         |
| GPIO 21 | W5500 INT         |
| GPIO 25 | Status LED        |
| GPIO 26 | Factory reset btn (active low, pull-up) |

---

## ekv Storage Schema

All keys written in lexicographically ascending order within transactions.

| Key                       | Value                   | Notes                        |
|---------------------------|-------------------------|------------------------------|
| `admin/first_login`       | `b"1"` or absent        | Cleared after first pw change |
| `init`                    | `b"1"`                  | Marks DB as initialized      |
| `p/{n}/name`              | UTF-8 ≤ 32 bytes        | Port name (n = 0–7)          |
| `u/{username}/admin`      | `b"1"` or `b"0"`        | Admin flag                   |
| `u/{username}/ports`      | 1 byte bitmask          | Allowed port bits 0–7        |
| `u/{username}/pw`         | 32 bytes (SHA-256)      | Password hash                |

Default seed (admin/admin, all ports):
- Password: SHA-256("admin"), changeable on first login
- Port bitmask: 0xFF (all 8 ports)

---

## REST API

| Method | Path                              | Auth     | Description                    |
|--------|-----------------------------------|----------|--------------------------------|
| GET    | `/`                               | any      | Serve index.html               |
| GET    | `/api/status`                     | any user | `{"version","ip","first_login"}`|
| GET    | `/api/sensors`                    | any user | Temperature + stubs            |
| GET    | `/api/gpio/:pin`                  | any user | Pin state + name               |
| POST   | `/api/gpio/:pin/toggle`           | any user | Toggle (ACL enforced)          |
| POST   | `/api/gpio/:pin/set`              | any user | Set state (ACL enforced)       |
| GET    | `/api/port/:n/name`               | any user | Get port name                  |
| POST   | `/api/port/:n/name`               | admin    | Set port name                  |
| POST   | `/api/admin/password`             | admin    | Change own password            |
| POST   | `/api/admin/users`                | admin    | Create user                    |
| POST   | `/api/admin/users/:user/ports`    | admin    | Set user port ACL              |
| DELETE | `/api/admin/users/:user`          | admin    | Delete user                    |
| POST   | `/api/admin/reset`                | admin    | Factory reset + reboot         |
| POST   | `/api/update`                     | admin    | OTA firmware upload            |

---

## Build System (xtask)

```
cargo xtask build                  # build bootloader + application
cargo xtask build --bootloader     # bootloader only
cargo xtask build --application    # application only
cargo xtask combine                # combine ELFs → build/combined.uf2
cargo xtask dist                   # build + combine (full release)

cargo xtask flash                  # flash combined via UF2 (BOOTSEL)
cargo xtask flash --bootloader     # flash bootloader only
cargo xtask flash --application    # flash application only
cargo xtask flash --probe          # use probe-rs instead of UF2
cargo xtask flash --ota <ip>       # OTA upload to running device

cargo xtask check-tools            # verify required tools
cargo xtask clean                  # clean all build artifacts
```

---

## Implementation Tasks

| # | Task                              | Status     | Progress file              |
|---|-----------------------------------|------------|----------------------------|
| 1 | Flash layout (memory.x files)     | ✅ Done     | `tasks/task-01-flash.md`   |
| 2 | Dependency upgrades               | ✅ Done     | `tasks/task-02-deps.md`    |
| 3 | xtask build system                | ⏳ Pending  | `tasks/task-03-xtask.md`   |
| 4 | Application core modules          | ⏳ Pending  | `tasks/task-04-core.md`    |
| 5 | picoserve web layer               | ⏳ Pending  | `tasks/task-05-web.md`     |
| 6 | Web UI rewrite                    | ⏳ Pending  | `tasks/task-06-ui.md`      |
| 7 | main.rs rewrite + CLAUDE.md       | ⏳ Pending  | `tasks/task-07-main.md`    |

---

## Dependencies (target versions)

| Crate              | Current           | Target                           |
|--------------------|-------------------|----------------------------------|
| embassy-rp         | 0.8.0 (git 539837a) | 0.9.0 (git 3651d8e)            |
| embassy-boot-rp    | 0.8.0 (crates.io) | 0.9.0 (git 3651d8e)             |
| embassy-net        | 0.7.1 (crates.io) | 0.8.0+ (git 3651d8e)            |
| embassy-net-wiznet | 0.2.1 (crates.io) | 0.2.1 (git 3651d8e)             |
| embassy-executor   | 0.9.1 (git 539837a) | (git 3651d8e)                  |
| embassy-sync       | 0.7.2 (git 539837a) | (git 3651d8e)                  |
| embassy-time       | 0.5.0 (git 539837a) | (git 3651d8e)                  |
| embedded-io-async  | 0.6.1             | 0.7                              |
| heapless           | 0.8 (app) / 0.9.1 (ws) | 0.8 (ekv constraint)       |
| picoserve          | (absent)          | 0.18 (features: embassy, defmt, json) |
| ekv                | (absent)          | 1.0 (page-size-4096, 64 pages)  |
| sha2               | (absent)          | 0.10 (no_std)                   |
| serde              | (absent)          | 1.0 (no_std, derive)            |
| rust-toolchain     | nightly-2025-02-01 | nightly-2026-02-01             |
