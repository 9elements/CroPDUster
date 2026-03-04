# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

This is a Rust embedded project for the Raspberry Pi Pico (RP2040) that implements a PDU (Power Distribution Unit) controller with the following features:

1. **Bootloader** (`bootloader/`): Uses `embassy-boot-rp` to manage firmware updates with flash partitioning
2. **Application** (`application/`): HTTP server with web-based GPIO control and OTA firmware updates

The bootloader enables safe over-the-air firmware updates by maintaining separate ACTIVE and DFU (Device Firmware Update) partitions. The application provides a complete web interface for GPIO control and supports firmware uploads via HTTP.

## Build Commands

### Using cargo xtask (Recommended)

```bash
# Build everything (bootloader + application)
cargo xtask build

# Build individual components
cargo xtask build --bootloader    # Build bootloader only
cargo xtask build --application   # Build main PDU controller application

# Produce combined UF2 (build + combine)
cargo xtask dist

# Flash to device
cargo xtask flash                         # Flash combined UF2 via BOOTSEL drag-and-drop
cargo xtask flash --bootloader            # Flash bootloader only
cargo xtask flash --application           # Flash application only
cargo xtask flash --ota 192.168.1.100     # OTA upload to running device

# Utility commands
cargo xtask clean        # Clean all build artifacts
cargo xtask check-tools  # Verify all required tools are installed
```

All build outputs are placed in the `build/` directory:
- `combined.uf2` - **Recommended** - Single file with bootloader + application (~280KB)
- `bootloader.uf2` - Bootloader firmware (16KB)
- `application.uf2` - Main PDU controller application (168KB)

### Manual Flashing

**Option 1: Combined Binary (Recommended for initial provisioning)**
1. Hold BOOTSEL button while connecting Pico to USB
2. Drag `build/combined.uf2` to the RPI-RP2 drive
3. Device will boot automatically with both bootloader and application

**Option 2: Separate Binaries**
1. Hold BOOTSEL button while connecting Pico to USB
2. Drag `build/bootloader.uf2` to the RPI-RP2 drive
3. Wait for device to reboot
4. Hold BOOTSEL button again
5. Drag `build/application.uf2` to the RPI-RP2 drive

## Prerequisites

- Rust nightly toolchain (2025-02-01)
- `thumbv6m-none-eabi` target
- `elf2uf2-rs` (UF2 converter)
- `flip-link` (linker for stack overflow protection)
- Optional: `cargo-flash` (for flashing to device)

## Hardware Configuration

### W5500 Ethernet Module (SPI)
- MISO: GPIO 16
- MOSI: GPIO 19
- CLK: GPIO 18
- CS: GPIO 17
- INT: GPIO 21
- RST: GPIO 20
- SPI Frequency: 50 MHz

### GPIO Outputs (PDU Control)
- GPIO 0: Relay/Output 0
- GPIO 1: Relay/Output 1
- GPIO 2: Relay/Output 2
- GPIO 3: Relay/Output 3

### Status LED
- GPIO 25: Built-in LED (indicates network status)

## Application Features

### Current Implementation

The application (`application/src/main.rs`) provides:
- **HTTP Server** on port 80 with web interface
- **Web Interface**: Browser-based GPIO control with toggle buttons
- **REST API**: JSON endpoints for GPIO control and status
- **Firmware Updates**: OTA updates via HTTP with A/B partition management
- **DHCP** client for automatic IP configuration
- **Watchdog Timer**: 8-second timeout with periodic feeding
- **Status LED**: Indicates network activity

### Usage Examples

#### Web Interface
1. Check serial console for DHCP-assigned IP address
2. Open browser to `http://<device-ip>/`
3. Click GPIO toggle buttons to control outputs
4. Upload firmware using the web interface

#### REST API
```bash
# Get device status
curl http://192.168.1.100/api/status
# Returns: {"status":"running","version":"1.0.0"}

# Toggle GPIO 0
curl -X POST http://192.168.1.100/api/gpio/0/toggle
# Returns: {"status":"ok"}

# Upload firmware (triggers reboot)
curl -X POST -H "Content-Type: application/octet-stream" \
  --data-binary @build/application.uf2 \
  http://192.168.1.100/api/update
# Returns: {"status":"ok","message":"Firmware uploaded, rebooting..."}
```

## Architecture

### Flash Memory Layout

The flash is partitioned as follows (see `memory.x` files):

- **BOOT2**: `0x10000000` - `0x100` (256 bytes) - RP2040 second-stage bootloader
- **Bootloader Flash**: `0x10000100` - 24KB - Bootloader code
- **BOOTLOADER_STATE**: `0x10006000` - 4KB - Bootloader state/metadata
- **ACTIVE**: `0x10007000` - 512KB - Currently running application
- **DFU**: `0x10087000` - 516KB - Staged firmware update

### Bootloader Operation

The bootloader (`bootloader/src/main.rs`) performs these operations on boot:

1. Initializes flash with watchdog timer (8 second timeout)
2. Reads configuration from linker-defined memory regions
3. Checks BOOTLOADER_STATE for pending updates
4. If update marked, copies DFU partition to ACTIVE
5. Jumps to ACTIVE partition to execute application

Hard faults trigger system reset to retry boot.

### Application Structure

The application uses Embassy async runtime with multiple concurrent tasks:
- **ethernet_task**: Manages W5500 hardware and link layer
- **net_task**: Runs the embassy-net network stack (TCP/IP, DHCP)
- **gpio_task**: Handles GPIO control commands via Signal primitive
- **main task**: Runs HTTP server loop on port 80
  - Parses HTTP requests (method, path, headers)
  - Routes to appropriate handlers
  - Serves embedded HTML web interface
  - Handles REST API endpoints
  - Streams firmware uploads to DFU partition

### HTTP Server Implementation

The application implements a lightweight HTTP/1.1 server without external dependencies:
- Manual HTTP request parsing
- Response building with proper headers
- Static content serving (embedded HTML)
- JSON API responses
- Streaming file upload handling

### Firmware Update Process

When firmware is uploaded via `/api/update`:
1. HTTP body is streamed to avoid large memory buffers
2. Data written to DFU partition in 4KB chunks using `FirmwareUpdater`
3. DFU partition marked as ready via `mark_updated()`
4. System reset triggered via `cortex_m::peripheral::SCB::sys_reset()`
5. Bootloader detects update on next boot and swaps partitions

## Cargo Workspace

This is a Cargo workspace with two members:

- `bootloader`: Bootloader binary (uses minimal dependencies)
- `application`: Main PDU controller application

### Key Dependencies

Application dependencies:
- `embassy-executor`: Async task executor
- `embassy-net`: Network stack with DHCP support
- `embassy-net-wiznet`: Driver for W5500 Ethernet chip
- `embassy-rp`: RP2040 HAL with flash support
- `embassy-boot-rp`: Bootloader and firmware updater
- `embassy-embedded-hal`: Async adapters for blocking peripherals
- `embassy-sync`: Synchronization primitives (Signal, Mutex)
- `embedded-hal-bus` v0.1: Async SPI device support
- `static_cell`: Static memory allocation
- `portable-atomic`: Atomic operations with critical-section support
- `heapless`: No-std collections (String, Vec)

Dependencies use patched Embassy framework from git revision `3651d8ef249...`.

## Development Workflow

1. **Initial Setup**: Flash bootloader (only once)
   ```bash
   cargo xtask flash --bootloader
   ```

2. **Flash Application**: Via USB or OTA
   ```bash
   # Option A: Flash via USB
   cargo xtask flash --application

   # Option B: Upload via HTTP (after first flash)
   cargo xtask flash --ota <device-ip>
   ```

3. **Monitor**: Connect to serial console
   ```bash
   screen /dev/ttyACM0 115200
   # Watch for DHCP IP assignment
   ```

4. **Control**: Open web browser or use REST API
   ```bash
   # Web interface
   open http://<device-ip>/

   # Or use curl
   curl -X POST http://<device-ip>/api/gpio/0/toggle
   ```

## Toolchain Configuration

- Nightly Rust channel: `nightly-2026-02-01`
- Target: `thumbv6m-none-eabi` (Cortex-M0+ architecture)
- Required components: `rust-src`, `rustfmt`, `llvm-tools`, `miri`
- Release profile: optimized for size (`opt-level = "s"`), LTO enabled

## Debugging

The `.cargo/config.toml` files configure:

- Linker: `flip-link` (stack overflow protection)
- Runner: `elf2uf2-rs` (default) or `probe-rs` (commented out)
- `DEFMT_LOG=debug` environment variable for logging

To use probe-rs instead of UF2 flashing, uncomment the probe-rs runner line in `.cargo/config.toml`.

## Implementation Notes

### HTTP Server Design Decisions

The application implements HTTP/1.1 directly over TCP without external HTTP libraries because:
- **Size**: Minimal code footprint (no picoserve dependency)
- **Control**: Full control over parsing and response generation
- **Streaming**: Easy to implement streaming uploads for large firmware files
- **Simplicity**: Straightforward to maintain and debug

Key implementation details:
- Manual HTTP request parsing using string operations
- Case-insensitive header matching for compatibility
- Streaming firmware uploads to minimize RAM usage
- Response building with heapless::Vec (16KB max)

### Memory Constraints

RP2040 has 264KB RAM total. Memory allocation strategy:
- HTTP buffers: 8KB RX + 8KB TX + 2KB parsing = 18KB
- Firmware write buffer: 4KB (ERASE_SIZE)
- Network stack resources: ~8KB
- GPIO and other tasks: minimal
- Total: ~30KB, well within limits

## Potential Enhancements

Future additions could include:
1. **Security**: Firmware signature verification, HTTP authentication
2. **Monitoring**: Real-time GPIO state WebSocket updates
3. **Protocols**: MQTT integration for IoT platforms
4. **Features**: Input monitoring, PWM support, sensor integration
