//! GPIO task and shared state for 8 relay outputs.

use embassy_rp::gpio::Output;
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::mutex::Mutex;
use embassy_sync::signal::Signal;

/// Commands sent to the GPIO task.
#[derive(Clone, Copy, Debug)]
pub enum GpioCommand {
    /// Toggle the output state of the given pin index (0–7).
    Toggle(u8),
    /// Set the output state of the given pin index (0–7) to the given level.
    Set(u8, bool),
}

/// Signal used to send commands to `gpio_task`.
pub static GPIO_SIGNAL: Signal<CriticalSectionRawMutex, GpioCommand> = Signal::new();

/// Shared GPIO output states (index 0–7).
pub static GPIO_STATES: Mutex<CriticalSectionRawMutex, [bool; 8]> = Mutex::new([false; 8]);

/// GPIO control task — manages 8 relay output pins.
///
/// Listens on `GPIO_SIGNAL` for commands and updates both the pin hardware
/// and the shared `GPIO_STATES` mutex.
#[embassy_executor::task]
pub async fn gpio_task(mut pins: [Output<'static>; 8]) {
    loop {
        let cmd = GPIO_SIGNAL.wait().await;

        let pin_index = match cmd {
            GpioCommand::Toggle(p) | GpioCommand::Set(p, _) => p as usize,
        };

        if pin_index >= 8 {
            continue;
        }

        // Determine new state and update the shared states map
        let new_state = {
            let mut states = GPIO_STATES.lock().await;
            let new_state = match cmd {
                GpioCommand::Toggle(_) => !states[pin_index],
                GpioCommand::Set(_, s) => s,
            };
            states[pin_index] = new_state;
            new_state
        };

        // Drive the hardware pin
        if new_state {
            pins[pin_index].set_high();
        } else {
            pins[pin_index].set_low();
        }
    }
}
