# Task 07 — main.rs Rewrite + CLAUDE.md Update

## Status: ⏳ Pending

## Objective
Replace the monolithic `application/src/main.rs` with a lean orchestrator that
initialises all peripherals, spawns all tasks, and wires together the modules
created in Tasks 04 and 05. Update CLAUDE.md to reflect the new build system.

## `application/src/main.rs` Structure

```rust
#![no_std]
#![no_main]
#![feature(impl_trait_in_assoc_type)]

mod config;
mod storage;
mod auth;
mod gpio;
mod sensors;
mod web;

#[embassy_executor::main]
async fn main(spawner: Spawner) {
    // 1. embassy_rp::init
    // 2. Factory reset check (PIN_26 held low)
    // 3. Init flash → ekv database
    // 4. Init GPIO_STATES (8 outputs)
    // 5. Init SENSOR_DATA
    // 6. Spawn gpio_task (8 pins)
    // 7. Init W5500 SPI + net stack
    // 8. Spawn ethernet_task, net_task
    // 9. Watchdog start
    // 10. Wait for DHCP
    // 11. Spawn sensor_task (ADC)
    // 12. Build picoserve app + state
    // 13. Spawn 4× web_task
    // 14. LED blink ready signal
}
```

## Peripheral Allocation
| Peripheral | Use |
|---|---|
| `p.FLASH` | ekv PduFlash |
| `p.WATCHDOG` | WatchdogFlash in bootloader (app creates separately) |
| `p.PIN_0`..`p.PIN_7` | gpio_task outputs |
| `p.PIN_16`..`p.PIN_21` | W5500 SPI |
| `p.PIN_25` | Status LED |
| `p.PIN_26` | Factory reset input |
| `p.SPI0` | W5500 |
| `p.DMA_CH0`, `p.DMA_CH1` | SPI DMA |
| `p.ADC` | sensor_task |

## `bind_interrupts!`
```rust
embassy_rp::bind_interrupts!(struct Irqs {
    ADC_IRQ_FIFO => embassy_rp::adc::InterruptHandler;
});
```

## CLAUDE.md Updates
- Replace all `make *` commands with `cargo xtask *`
- Update GPIO table from 4 to 8 outputs
- Update flash layout table
- Update dependency list
- Remove Makefile references

## Checklist
- [ ] Rewrite `application/src/main.rs`
- [ ] Add `mod` declarations for all new modules
- [ ] Wire factory reset GPIO check
- [ ] Wire ekv database init with format-on-corruption fallback
- [ ] Wire all tasks
- [ ] Update `CLAUDE.md`

## Log
<!-- Agent fills this in -->
