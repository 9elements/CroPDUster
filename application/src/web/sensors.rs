//! GET /api/sensors handler.

use picoserve::response::Json;
use serde::Serialize;

use crate::sensors::SENSOR_DATA;

#[derive(Serialize)]
pub struct SensorsResponse {
    pub temperature_c: f32,
    pub voltage_v: f32,
    pub current_a: [f32; 8],
}

/// `GET /api/sensors` — returns temperature, voltage, and per-port current stubs.
pub async fn handle_sensors() -> Json<SensorsResponse> {
    let data = {
        let guard = SENSOR_DATA.lock().await;
        *guard
    };
    Json(SensorsResponse {
        temperature_c: data.temperature_c,
        voltage_v: data.voltage_v,
        current_a: data.current_a,
    })
}
