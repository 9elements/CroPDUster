//! RP2040 internal temperature sensor and HLW8012 power/current/voltage sensor.

use embassy_rp::adc::{Adc, Async, Channel};
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::mutex::Mutex;
use embassy_time::{Duration, Timer};

use crate::hlw8012::Hlw8012Driver;

/// Snapshot of all sensor readings.
#[derive(Clone, Copy, Debug)]
pub struct SensorData {
    /// Die temperature in degrees Celsius (RP2040 internal ADC).
    pub temperature_c: f32,
    /// Mains voltage RMS in Volts (HLW8012 CF1, voltage mode).
    pub voltage_v: f32,
    /// Total PDU input current RMS in Amps (HLW8012 CF1, current mode).
    pub current_total_a: f32,
    /// Active power in Watts (HLW8012 CF).
    pub power_w: f32,
}

impl SensorData {
    pub const fn default_val() -> Self {
        Self {
            temperature_c: 0.0,
            voltage_v: 0.0,
            current_total_a: 0.0,
            power_w: 0.0,
        }
    }
}

/// Shared sensor readings, updated periodically by `sensor_task`.
pub static SENSOR_DATA: Mutex<CriticalSectionRawMutex, SensorData> =
    Mutex::new(SensorData::default_val());

/// Sensor polling task.
///
/// Reads the RP2040 internal temperature sensor and all three HLW8012
/// outputs (power, current, voltage) in a loop, then updates `SENSOR_DATA`.
///
/// One full measurement cycle takes approximately:
///   - Temperature:   immediate (ADC)
///   - Power (CF):    up to `PULSE_TIMEOUT_MS` (or one pulse period ≪ that)
///   - Current (CF1): up to `PULSE_TIMEOUT_MS`
///   - Voltage (CF1): `SEL_SETTLE_MS` + up to `PULSE_TIMEOUT_MS`
///   - Sleep:         5 s
///
/// With default constants the worst-case cycle is about 17 s (two 10 s
/// timeouts + 2 s settle + 5 s sleep), which only occurs at zero load.
/// Under any non-zero load all three HLW8012 reads complete within a single
/// pulse period (≪ 1 s at typical loads).
#[embassy_executor::task]
pub async fn sensor_task(
    mut adc: Adc<'static, Async>,
    mut ts_channel: Channel<'static>,
    mut hlw: Hlw8012Driver<'static>,
) {
    loop {
        // ── 1. RP2040 internal temperature (ADC) ──────────────────────────
        let raw = adc.read(&mut ts_channel).await.unwrap_or(0);
        // RP2040 datasheet formula: T = 27 − (V_adc − 0.706) / 0.001721
        let voltage = (raw as f32) * 3.3 / 4096.0;
        let temp_c = 27.0 - (voltage - 0.706) / 0.001721;

        // ── 2. HLW8012 active power (CF, SM0) ─────────────────────────────
        let power_w = hlw.read_power_w().await;

        // ── 3. HLW8012 current RMS (CF1, SEL=LOW, SM1) ────────────────────
        // SEL is already LOW from the previous cycle's read_voltage_v() call
        // (or from initialisation).
        let current_total_a = hlw.read_current_a().await;

        // ── 4. HLW8012 voltage RMS (CF1, SEL toggled HIGH then LOW) ───────
        // read_voltage_v() sets SEL=HIGH, waits for stabilisation, reads,
        // then restores SEL=LOW ready for the next current measurement.
        let voltage_v = hlw.read_voltage_v().await;

        // ── 5. Publish ─────────────────────────────────────────────────────
        {
            let mut data = SENSOR_DATA.lock().await;
            data.temperature_c = temp_c;
            data.power_w = power_w;
            data.current_total_a = current_total_a;
            data.voltage_v = voltage_v;
        }

        Timer::after(Duration::from_secs(5)).await;
    }
}
