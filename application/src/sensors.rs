//! RP2040 internal temperature sensor and stub current/voltage sensors.

use embassy_rp::adc::{Adc, Async, Channel};
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::mutex::Mutex;
use embassy_time::{Duration, Timer};

/// Snapshot of sensor readings.
#[derive(Clone, Copy, Debug)]
pub struct SensorData {
    /// Die temperature in degrees Celsius (from internal ADC sensor).
    pub temperature_c: f32,
    /// DC bus voltage in Volts (stub — always 0.0 until hardware is wired).
    pub voltage_v: f32,
    /// Per-port current in Amps (stubs — always 0.0).
    pub current_a: [f32; 8],
}

impl SensorData {
    pub const fn default_val() -> Self {
        Self {
            temperature_c: 0.0,
            voltage_v: 0.0,
            current_a: [0.0; 8],
        }
    }
}

/// Shared sensor readings, updated every 5 seconds by `sensor_task`.
pub static SENSOR_DATA: Mutex<CriticalSectionRawMutex, SensorData> =
    Mutex::new(SensorData::default_val());

/// Sensor polling task.
///
/// Reads the RP2040 internal temperature sensor via ADC and updates
/// `SENSOR_DATA` every 5 seconds.
///
/// `adc`        — ADC driver in async mode (takes ownership).  
/// `ts_channel` — ADC channel for the internal temperature sensor
///                (created with `Channel::new_temp_sensor`).
#[embassy_executor::task]
pub async fn sensor_task(mut adc: Adc<'static, Async>, mut ts_channel: Channel<'static>) {
    loop {
        let raw = adc.read(&mut ts_channel).await.unwrap_or(0);

        // RP2040 datasheet formula:
        // T = 27 − (ADC_voltage − 0.706) / 0.001721
        let voltage = (raw as f32) * 3.3 / 4096.0;
        let temp_c = 27.0 - (voltage - 0.706) / 0.001721;

        {
            let mut data = SENSOR_DATA.lock().await;
            data.temperature_c = temp_c;
            // voltage_v and current_a remain stub 0.0 values
        }

        Timer::after(Duration::from_secs(5)).await;
    }
}
