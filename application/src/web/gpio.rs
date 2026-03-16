//! GPIO REST API handlers.
//!
//! GET  /api/gpio/:pin  — state + name + allowed flag
//! POST /api/gpio/:pin/toggle — toggle (ACL check)
//! POST /api/gpio/:pin/set    — set state (ACL check), body: `{"state":bool}`

use picoserve::extract::Json as JsonBody;
use picoserve::response::Json;
use serde::{Deserialize, Serialize};

use crate::auth::AuthUser;
use crate::gpio::{GpioCommand, GPIO_SIGNAL, GPIO_STATES};
use crate::storage::port_name_key;

// ── Response types ─────────────────────────────────────────────────────────────

#[derive(Serialize)]
pub struct GpioStateResponse {
    pub pin: u8,
    pub state: bool,
    pub name: heapless::String<16>,
    pub allowed: bool,
}

#[derive(Serialize)]
pub struct GpioToggleResponse {
    pub pin: u8,
    pub state: bool,
}

#[derive(Deserialize)]
pub struct SetBody {
    pub state: bool,
}

// ── Helpers ────────────────────────────────────────────────────────────────────

async fn read_port_name(pin: u8) -> heapless::String<16> {
    let key = port_name_key(pin);
    let mut buf = [0u8; 16];
    let db = crate::web::db();
    let rtx = db.read_transaction().await;
    if let Ok(n) = rtx.read(&key, &mut buf).await {
        heapless::String::from_utf8(heapless::Vec::from_slice(&buf[..n]).unwrap_or_default())
            .unwrap_or_default()
    } else {
        heapless::String::new()
    }
}

// ── GET /api/gpio/:pin ─────────────────────────────────────────────────────────

pub async fn handle_gpio_get(pin: u8, user: AuthUser) -> Json<GpioStateResponse> {
    let state_val = {
        let states = GPIO_STATES.lock().await;
        if (pin as usize) < 8 {
            states[pin as usize]
        } else {
            false
        }
    };
    let name = read_port_name(pin).await;
    let allowed = user.allowed_ports & (1 << pin) != 0;
    Json(GpioStateResponse {
        pin,
        state: state_val,
        name,
        allowed,
    })
}

// ── POST /api/gpio/:pin/toggle ─────────────────────────────────────────────────

pub async fn handle_gpio_toggle(pin: u8, user: AuthUser) -> Json<GpioToggleResponse> {
    if pin >= 8 || (user.allowed_ports & (1 << pin) == 0) {
        let state_val = {
            let states = GPIO_STATES.lock().await;
            if (pin as usize) < 8 {
                states[pin as usize]
            } else {
                false
            }
        };
        return Json(GpioToggleResponse {
            pin,
            state: state_val,
        });
    }

    GPIO_SIGNAL.signal(GpioCommand::Toggle(pin));
    embassy_time::Timer::after_millis(1).await;
    let state_val = {
        let states = GPIO_STATES.lock().await;
        states[pin as usize]
    };

    Json(GpioToggleResponse {
        pin,
        state: state_val,
    })
}

// ── POST /api/gpio/:pin/set ────────────────────────────────────────────────────

pub async fn handle_gpio_set(
    pin: u8,
    user: AuthUser,
    JsonBody(body): JsonBody<SetBody>,
) -> Json<GpioToggleResponse> {
    if pin >= 8 || (user.allowed_ports & (1 << pin) == 0) {
        let state_val = {
            let states = GPIO_STATES.lock().await;
            if (pin as usize) < 8 {
                states[pin as usize]
            } else {
                false
            }
        };
        return Json(GpioToggleResponse {
            pin,
            state: state_val,
        });
    }

    GPIO_SIGNAL.signal(GpioCommand::Set(pin, body.state));
    embassy_time::Timer::after_millis(1).await;
    let state_val = {
        let states = GPIO_STATES.lock().await;
        states[pin as usize]
    };

    Json(GpioToggleResponse {
        pin,
        state: state_val,
    })
}
