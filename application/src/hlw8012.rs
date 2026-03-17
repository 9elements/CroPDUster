//! HLW8012 single-phase energy meter driver (PIO-based).
//!
//! # Protocol
//!
//! The HLW8012 outputs three digital signals to the MCU:
//!
//! | Pin | Direction | Description |
//! |-----|-----------|-------------|
//! | CF  | Output    | 50% duty-cycle square wave; frequency ∝ active power |
//! | CF1 | Output    | 50% duty-cycle square wave; frequency ∝ current RMS **or** voltage RMS |
//! | SEL | Input     | LOW = CF1 outputs current RMS, HIGH = CF1 outputs voltage RMS |
//!
//! # Measurement principle
//!
//! This driver uses two RP2040 PIO state machines to measure the HIGH
//! half-period of each output.  Since the duty cycle is exactly 50%:
//!
//! ```text
//! full_period = 2 × T_high
//! frequency   = 1 / full_period = 1 / (2 × T_high)
//! ```
//!
//! # Calibration multipliers (µs · unit⁻¹)
//!
//! Following the convention of the widely-used Arduino HLW8012 library, the
//! driver stores pre-computed multipliers M such that:
//!
//! ```text
//! physical_value = M / T_half_µs / 2
//! ```
//!
//! This is numerically identical to `M * frequency` scaled appropriately.
//! The multipliers are derived by inverting the datasheet §3.2 formulas and
//! converting from seconds to microseconds (factor 1 × 10⁶):
//!
//! ```text
//! current_mult = 1e6 × 512 × VREF / (24 × fosc × R_sense)
//! voltage_mult = 1e6 × 512 × VREF × V_ratio / (2 × fosc)
//! power_mult   = 1e6 × 128 × VREF² × V_ratio / (48 × fosc × R_sense)
//! ```
//!
//! # Accuracy caveat
//!
//! The HLW8012's built-in oscillator has ±15 % tolerance (3.04–4.12 MHz).
//! The nominal multipliers derived from the datasheet formula can therefore be
//! off by up to ±15 % chip-to-chip.  Calibration against a known load can be
//! performed by adjusting `HLW8012_R_SENSE` and `HLW8012_V_RATIO` in
//! `config.rs` until measurements agree with a reference instrument.
//!
//! # SEL stabilisation
//!
//! After toggling SEL, the CF1 output requires approximately 2 s to stabilise.
//! This was empirically confirmed by multiple users of the Arduino library and
//! is the reason `HLW8012_SEL_SETTLE_MS` defaults to 2 000 ms.
//!
//! # PIO program
//!
//! Both state machines share an identical program that measures the HIGH
//! half-period of a 50% duty-cycle square wave:
//!
//! ```asm
//! .wrap_target
//!     wait 1 pin 0       ; block until rising edge
//!     mov x, ~null       ; x = 0xFFFF_FFFF (count-down start)
//! count_loop:
//!     jmp pin count_dec  ; pin still HIGH → keep counting
//!     jmp count_done     ; pin went LOW   → stop
//! count_dec:
//!     jmp x-- count_loop ; x--, loop back (falls through on x == 0 overflow)
//! count_done:
//!     mov isr, x         ; store count register
//!     push noblock       ; push to RX FIFO (drop if full — never stalls SM)
//!     wait 0 pin 0       ; sync: wait for pin low before next wrap
//! .wrap
//! ```
//!
//! Each `count_loop` iteration consumes **2 PIO clock cycles** (1 for
//! `jmp pin` + 1 for `jmp x--`).  Converting raw count to time:
//!
//! ```text
//! delta      = !x_raw          (bitwise NOT = number of loop iterations)
//! T_half_µs  = delta × 2 / 125  (µs at 125 MHz system clock)
//! ```

use embassy_rp::gpio::Output;
use embassy_rp::peripherals::PIO0;
use embassy_rp::pio::program::pio_asm;
use embassy_rp::pio::{Common, Config, Pin as PioHwPin, PioPin, StateMachine};
use embassy_rp::Peri;
use embassy_time::{with_timeout, Duration, Timer};

// ── Chip-fixed physical constants (datasheet §3.2, §3.4, §3.5) ───────────────

/// HLW8012 built-in voltage reference [V] (typ.).
const VREF: f32 = 2.43;

/// HLW8012 built-in oscillator frequency [Hz] (typ.).
const F_OSC: f32 = 3_579_000.0;

// ── PIO timing ────────────────────────────────────────────────────────────────

/// RP2040 system clock [Hz].
const SYS_CLK_HZ: f32 = 125_000_000.0;

/// PIO clock cycles consumed per `count_loop` iteration while pin is HIGH.
///   `jmp pin count_dec` (1 cycle) + `jmp x-- count_loop` (1 cycle) = 2.
const CYCLES_PER_LOOP: f32 = 2.0;

// ── Spurious-pulse guard ──────────────────────────────────────────────────────

/// Pulse frequencies below this threshold are clamped to 0.0.
///
/// At 0.1 % of the HLW8012's full-scale input the output frequency is already
/// sub-Hz, so anything below 0.1 Hz is treated as noise rather than signal.
/// This guards against the occasional spurious pulse at no-load that multiple
/// users of the Arduino reference library have reported (issues #11, #27).
const MIN_FREQ_HZ: f32 = 0.1;

// ── Driver ────────────────────────────────────────────────────────────────────

/// Calibration and timing configuration for [`Hlw8012Driver`].
pub struct Hlw8012Config {
    /// Current shunt resistance in Ω (e.g. `0.001` for 1 mΩ).
    pub r_sense: f32,
    /// Voltage divider ratio: `(R_upstream + R_downstream) / R_downstream`.
    pub v_ratio: f32,
    /// Milliseconds to wait for a pulse before returning 0.0 (10 000 recommended).
    pub timeout_ms: u64,
    /// Milliseconds to wait after toggling SEL before reading CF1 (2 000 minimum).
    pub sel_settle_ms: u64,
}

/// PIO-based driver for the HLW8012 single-phase energy metering IC.
///
/// Tied to **PIO0** (SM0 = CF, SM1 = CF1).  PIO1 remains free for other use.
pub struct Hlw8012Driver<'d> {
    cf_sm: StateMachine<'d, PIO0, 0>,
    cf1_sm: StateMachine<'d, PIO0, 1>,
    // Kept alive so the PIO reference-count for these pins stays non-zero
    // while the state machines are running.  Dropping the Pin objects while
    // the SMs are active would not break anything with the current embassy-rp
    // implementation, but retaining them makes the ownership intent explicit.
    _cf_pin: PioHwPin<'d, PIO0>,
    _cf1_pin: PioHwPin<'d, PIO0>,
    sel: Output<'d>,
    current_multiplier: f32, // µs/A
    voltage_multiplier: f32, // µs/V
    power_multiplier: f32,   // µs/W
    timeout: Duration,
    sel_settle: Duration,
}

impl<'d> Hlw8012Driver<'d> {
    /// Initialise the driver.
    ///
    /// Loads the period-measurement PIO program into SM0 (CF) and SM1 (CF1)
    /// of PIO0, then starts both state machines.
    ///
    /// # Parameters
    ///
    /// * `common`  — mutable reference to the PIO0 common block.
    /// * `sm0` / `sm1` — state machines 0 and 1 of PIO0.
    /// * `cf_pin`  — GPIO pin wired to HLW8012 CF output (active power).
    /// * `cf1_pin` — GPIO pin wired to HLW8012 CF1 output (current/voltage).
    /// * `sel`     — GPIO output wired to HLW8012 SEL input.
    /// * `config`  — calibration and timing parameters (see [`Hlw8012Config`]).
    pub fn new<C, C1>(
        common: &mut Common<'d, PIO0>,
        sm0: StateMachine<'d, PIO0, 0>,
        sm1: StateMachine<'d, PIO0, 1>,
        cf_pin: Peri<'d, C>,
        cf1_pin: Peri<'d, C1>,
        sel: Output<'d>,
        config: Hlw8012Config,
    ) -> Self
    where
        C: PioPin + 'd,
        C1: PioPin + 'd,
    {
        // ── PIO program ────────────────────────────────────────────────────
        // Measures the HIGH half-period of the input pin by counting down X
        // from 0xFFFF_FFFF at 2 PIO clock cycles per loop iteration.
        // The RX FIFO receives X after the pin falls; recover elapsed count
        // via bitwise NOT: delta = !x_raw.
        let prg = pio_asm!(
            ".wrap_target",
            "    wait 1 pin 0", // wait for rising edge
            "    mov x, ~null", // x = 0xFFFF_FFFF
            "count_loop:",
            "    jmp pin count_dec", // pin HIGH  → go to decrement
            "    jmp count_done",    // pin LOW   → stop
            "count_dec:",
            "    jmp x-- count_loop", // x--; loop (falls through on x overflow)
            "count_done:",
            "    mov isr, x",   // store count
            "    push noblock", // push to RX FIFO; drop if full (never stalls)
            "    wait 0 pin 0", // sync: wait for pin low before next wrap
            ".wrap",
        );

        let loaded = common.load_program(&prg.program);

        // ── SM0: CF (active power) ─────────────────────────────────────────
        let cf_pio_pin = common.make_pio_pin(cf_pin);
        let mut cfg0 = Config::default();
        cfg0.use_program(&loaded, &[]);
        cfg0.set_in_pins(&[&cf_pio_pin]);
        cfg0.set_jmp_pin(&cf_pio_pin);
        let mut sm0 = sm0;
        sm0.set_config(&cfg0);
        sm0.set_enable(true);

        // ── SM1: CF1 (current / voltage RMS) ──────────────────────────────
        let cf1_pio_pin = common.make_pio_pin(cf1_pin);
        let mut cfg1 = Config::default();
        cfg1.use_program(&loaded, &[]);
        cfg1.set_in_pins(&[&cf1_pio_pin]);
        cfg1.set_jmp_pin(&cf1_pio_pin);
        let mut sm1 = sm1;
        sm1.set_config(&cfg1);
        sm1.set_enable(true);

        // ── Pre-compute calibration multipliers (µs/unit) ─────────────────
        // Inverted from datasheet §3.2 frequency formulas with ×1e6 to use µs.
        // Formula origin (current as example):
        //   F_CFI = V1 × 24 / VREF × fosc/512
        //   V1    = F_CFI × VREF × 512 / (24 × fosc)
        //   I     = V1 / R_sense
        //   I     = (1e6 / T_half_µs) × VREF × 512 / (24 × fosc × R_sense)
        //         = current_multiplier / T_half_µs
        //   (the extra /2 in the call site converts T_half to T_full period)
        let current_multiplier = 1_000_000.0 * 512.0 * VREF / (24.0 * F_OSC * config.r_sense);
        let voltage_multiplier = 1_000_000.0 * 512.0 * VREF * config.v_ratio / (2.0 * F_OSC);
        let power_multiplier =
            1_000_000.0 * 128.0 * VREF * VREF * config.v_ratio / (48.0 * F_OSC * config.r_sense);

        Self {
            cf_sm: sm0,
            cf1_sm: sm1,
            _cf_pin: cf_pio_pin,
            _cf1_pin: cf1_pio_pin,
            sel,
            current_multiplier,
            voltage_multiplier,
            power_multiplier,
            timeout: Duration::from_millis(config.timeout_ms),
            sel_settle: Duration::from_millis(config.sel_settle_ms),
        }
    }

    /// Read the CF output and return active power in Watts.
    ///
    /// Returns `0.0` if no pulse arrives within the configured timeout —
    /// i.e., no load or load below the HLW8012's anti-creep threshold.
    pub async fn read_power_w(&mut self) -> f32 {
        read_value::<0>(&mut self.cf_sm, self.power_multiplier, self.timeout).await
    }

    /// Read the CF1 output in current mode (SEL already LOW) and return
    /// total input current RMS in Amps.
    ///
    /// Returns `0.0` on timeout.
    pub async fn read_current_a(&mut self) -> f32 {
        read_value::<1>(&mut self.cf1_sm, self.current_multiplier, self.timeout).await
    }

    /// Switch SEL HIGH, wait for stabilisation, read voltage RMS from CF1,
    /// then restore SEL LOW.
    ///
    /// The stabilisation delay is `HLW8012_SEL_SETTLE_MS` (default 2 s).
    ///
    /// Returns `0.0` on timeout.
    pub async fn read_voltage_v(&mut self) -> f32 {
        self.sel.set_high();
        Timer::after(self.sel_settle).await;
        let v = read_value::<1>(&mut self.cf1_sm, self.voltage_multiplier, self.timeout).await;
        self.sel.set_low();
        v
    }
}

// ── Internal helpers ──────────────────────────────────────────────────────────

/// Wait for one half-period measurement from a PIO state machine's RX FIFO
/// and convert it to a physical value.
///
/// # Conversion
///
/// The PIO program decrements X once per 2 clock cycles while the pin is HIGH.
/// After the pin falls low:
///
/// ```text
/// delta     = !x_raw                              (loop iterations elapsed)
/// T_half_µs = delta × CYCLES_PER_LOOP / 125.0    (µs at 125 MHz)
/// value     = multiplier / T_half_µs / 2          (same formula as C++ library)
/// ```
///
/// Returns `0.0` on timeout or if the measured frequency is below
/// `MIN_FREQ_HZ` (spurious no-load pulse guard).
async fn read_value<const SM: usize>(
    sm: &mut StateMachine<'_, PIO0, SM>,
    multiplier: f32,
    timeout: Duration,
) -> f32 {
    let raw = match with_timeout(timeout, sm.rx().wait_pull()).await {
        Ok(v) => v,
        Err(_) => return 0.0, // timeout → no load / below detection threshold
    };

    // Recover loop iteration count via bitwise NOT.
    let delta = (!raw) as f32;
    if delta == 0.0 {
        // Counter overflowed: signal held HIGH for the entire 32-bit range
        // without going low — impossible at any reasonable frequency but handle
        // it gracefully.
        return 0.0;
    }

    // HIGH half-period in microseconds.
    let t_half_us = delta * CYCLES_PER_LOOP / (SYS_CLK_HZ / 1_000_000.0);

    // Frequency of the full square wave (full period = 2 × T_half).
    let freq_hz = 1_000_000.0 / (t_half_us * 2.0);

    // Clamp spurious very-low-frequency pulses to zero.
    if freq_hz < MIN_FREQ_HZ {
        return 0.0;
    }

    // Physical value: matches the Arduino library's formula exactly.
    multiplier / t_half_us / 2.0
}
