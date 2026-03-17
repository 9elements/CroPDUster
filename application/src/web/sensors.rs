//! GET /api/sensors handler.

use picoserve::response::Json;
use serde::Serialize;

use crate::sensors::SENSOR_DATA;

#[derive(Serialize)]
pub struct SensorsResponse {
    pub temperature_c: f32,
    pub voltage_v: f32,
    pub current_total_a: f32,
    pub power_w: f32,
}

/// `GET /api/sensors` — returns temperature, mains voltage, total current, and active power.
pub async fn handle_sensors() -> Json<SensorsResponse> {
    let data = {
        let guard = SENSOR_DATA.lock().await;
        *guard
    };
    Json(SensorsResponse {
        temperature_c: data.temperature_c,
        voltage_v: data.voltage_v,
        current_total_a: data.current_total_a,
        power_w: data.power_w,
    })
}
