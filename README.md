# CroPDUster

Network-attached PDU (Power Distribution Unit) controller for the
Raspberry Pi Pico (RP2040), replacing the original PIC18-based firmware.

## Features

- 8 relay-controlled power outlets (GPIO 0-7)
- W5500 Ethernet with DHCP
- HTTP server with single-page web UI
- REST API for GPIO control, sensors, user management
- HTTP Basic Auth with multi-user support and per-port ACL
- OTA firmware updates via HTTP (A/B partition with embassy-boot)
- Persistent configuration via ekv key-value store
- Factory reset via GPIO 26 or admin API

## Quick Start

```bash
# Build everything
cargo xtask dist

# Flash via probe-rs (recommended for development)
cargo xtask flash --probe

# Or flash via UF2 drag-and-drop
# Hold BOOTSEL, connect USB, drag build/combined.uf2 to RPI-RP2

# OTA update a running device
cargo xtask flash --ota <device-ip>
```

See [CLAUDE.md](CLAUDE.md) for detailed build instructions, architecture,
and API documentation.

## Prerequisites

- Rust nightly (2026-02-01) with `thumbv6m-none-eabi` target
- `elf2uf2-rs`, `flip-link`
- Optional: `probe-rs` for probe-based flashing and RTT logging

## Project Structure

```
application/   Rust application (HTTP server, GPIO, auth, web UI)
bootloader/    embassy-boot-rp bootloader
xtask/         Build/flash helper (cargo xtask ...)
archive/       Original PIC18 firmware tooling (reference only)
```

## Trivia

A cropduster is a type of old agricultural airplane used to supply crops
with fertilizers. Not only is it one of the only words in the dictionary
having the letters PDU in it to form a nice pun, but it also easy to
draw a logo for it.

## License

BSD 3-Clause -- see [LICENSE](LICENSE).
