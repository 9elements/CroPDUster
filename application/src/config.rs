//! Compile-time constants for the PDU controller.
#![allow(unused)]

pub const FLASH_SIZE: usize = 2 * 1024 * 1024; // 2MB

// Flash layout (must match memory.x)
pub const ACTIVE_START: u32 = 0x10007000;
pub const ACTIVE_SIZE: u32 = 320 * 1024;
pub const DFU_START: u32 = 0x10057000;
pub const DFU_SIZE: u32 = 324 * 1024;
pub const CONFIG_START: u32 = 0x100A8000;
pub const CONFIG_SIZE: u32 = 256 * 1024;

// ekv: 256KB / 4096 bytes per page = 64 pages
pub const EKV_PAGE_SIZE: usize = 4096;
pub const EKV_PAGE_COUNT: usize = 64;

// PDU
pub const PORT_COUNT: usize = 8;
pub const MAX_USERS: usize = 8;

// GPIO pins
pub const PIN_RELAY_0: u8 = 0;
pub const PIN_RELAY_1: u8 = 1;
pub const PIN_RELAY_2: u8 = 2;
pub const PIN_RELAY_3: u8 = 3;
pub const PIN_RELAY_4: u8 = 4;
pub const PIN_RELAY_5: u8 = 5;
pub const PIN_RELAY_6: u8 = 6;
pub const PIN_RELAY_7: u8 = 7;
pub const PIN_LED: u8 = 25;
pub const PIN_FACTORY_RESET: u8 = 26;

// HLW8012 power monitor (PIO0, SM0=CF, SM1=CF1)
pub const PIN_HLW_CF: u8 = 8; // CF  — active-power pulse input
pub const PIN_HLW_CF1: u8 = 9; // CF1 — current/voltage RMS pulse input
pub const PIN_HLW_SEL: u8 = 10; // SEL — output: LOW=current mode, HIGH=voltage mode

// HLW8012 circuit calibration
// v_ratio = (R_upstream + R_downstream) / R_downstream
// Datasheet §3.1 example: 4×470 kΩ upstream + 1 kΩ downstream → (1880+1)/1 = 1881
pub const HLW8012_R_SENSE: f32 = 0.001; // 1 mΩ current shunt [Ω]
pub const HLW8012_V_RATIO: f32 = 1881.0; // voltage divider ratio (dimensionless)
                                         // Minimum wait after toggling SEL before CF1 readings stabilise.
                                         // 2 s is the empirical minimum; matches the Arduino reference library default.
pub const HLW8012_SEL_SETTLE_MS: u64 = 2_000;
// Treat readings as zero if no pulse arrives within this window.
// At 0.1 % of rated full-scale the period is ~5 s; 10 s gives comfortable margin.
pub const HLW8012_PULSE_TIMEOUT_MS: u64 = 10_000;

// W5500 SPI0
pub const PIN_MISO: u8 = 16;
pub const PIN_CS: u8 = 17;
pub const PIN_CLK: u8 = 18;
pub const PIN_MOSI: u8 = 19;
pub const PIN_RST: u8 = 20;
pub const PIN_INT: u8 = 21;
pub const SPI_FREQ_HZ: u32 = 50_000_000;
