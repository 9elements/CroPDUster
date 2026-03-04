# Task 07 — main.rs Rewrite + CLAUDE.md Update

## Status: ✅ Done

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
- [x] Rewrite `application/src/main.rs`
- [x] Add `mod` declarations for all new modules
- [x] Wire factory reset GPIO check
- [x] Wire ekv database init with format-on-corruption fallback
- [x] Wire all tasks
- [x] Update `CLAUDE.md`

## Log

`application/src/main.rs` rewritten. All 14 init steps implemented per spec:
1. `embassy_rp::init` — HAL init
2. Factory reset check on PIN_26 (pulled up; held low at boot triggers format+seed)
3. Async Flash + ekv database init via `init_database` (DMA_CH2)
4–6. 8 GPIO output pins + `gpio_task` spawn
7–8. W5500 SPI0 (DMA_CH0 TX, DMA_CH1 RX) + ethernet/net task spawns
9. Watchdog 8 s
10. DHCP wait loop (feeds watchdog while waiting)
11. ADC sensor task spawn (internal temp sensor)
12. `App.build_app()` — stateless picoserve router
13. 4× `web_task` spawns
14. LED blink + main watchdog-feed loop

Notable fixes applied during this task:
- `bind_interrupts!`: DMA_IRQ_0 maps multiple handlers (`DMA_CH0`, `DMA_CH1`, `DMA_CH2`) on one line
- `Spi::new` takes 8 args: `(spi, clk, mosi, miso, tx_dma, rx_dma, irq, config)`
- `Flash::new` async: `(flash, dma_ch, irq)` — 3 args
- `spawner.spawn(task_fn(...).unwrap())` pattern (task macro returns `Result<SpawnToken, SpawnError>`)
- `#![recursion_limit = "256"]` to handle `TaskPool` layout depth
- `RoscRng::next_u32()` / `next_u64()` are inherent methods — no `rand_core::RngCore` import needed
- Workspace `Cargo.toml` `cortex-m` feature corrected from `"critical-section"` (invalid) to `"inline-asm"` only
