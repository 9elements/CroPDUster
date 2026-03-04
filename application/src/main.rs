//! PDU Controller — lean orchestrator.
//!
//! Initialises all peripherals, spawns all tasks, and wires together the
//! modules created in Tasks 04 and 05.

#![no_std]
#![no_main]
#![feature(impl_trait_in_assoc_type)]
#![recursion_limit = "256"]

mod auth;
mod config;
mod gpio;
mod sensors;
mod storage;
mod web;

use defmt::*;
use embassy_executor::Spawner;
use embassy_net::StackResources;
use embassy_net_wiznet::chip::W5500;
use embassy_net_wiznet::*;
use embassy_rp::adc::{Adc, Channel, Config as AdcConfig};
use embassy_rp::bind_interrupts;
use embassy_rp::clocks::RoscRng;
use embassy_rp::dma;
use embassy_rp::flash::{Async as FlashAsync, Flash};
use embassy_rp::gpio::{Input, Level, Output, Pull};
use embassy_rp::peripherals::SPI0;
use embassy_rp::spi::{Async, Config as SpiConfig, Spi};
use embassy_rp::watchdog::Watchdog;
use embassy_time::{Duration, Timer};
use embedded_hal_bus::spi::ExclusiveDevice;
use picoserve::AppWithStateBuilder;
use static_cell::StaticCell;
use {defmt_rtt as _, panic_reset as _};

use config::{FLASH_SIZE, SPI_FREQ_HZ};
use gpio::gpio_task;
use sensors::sensor_task;
use storage::init_database;
use web::{web_task, App, CONFIG, WEB_TASK_POOL_SIZE};

// ── Interrupt bindings ─────────────────────────────────────────────────────────

bind_interrupts!(struct Irqs {
    // ADC — internal temperature sensor
    ADC_IRQ_FIFO => embassy_rp::adc::InterruptHandler;
    // DMA — SPI (CH0 TX, CH1 RX) + Flash (CH2); all share DMA_IRQ_0
    DMA_IRQ_0 => dma::InterruptHandler<embassy_rp::peripherals::DMA_CH0>,
                 dma::InterruptHandler<embassy_rp::peripherals::DMA_CH1>,
                 dma::InterruptHandler<embassy_rp::peripherals::DMA_CH2>;
});

// ── Ethernet task ──────────────────────────────────────────────────────────────

#[embassy_executor::task]
async fn ethernet_task(
    runner: Runner<
        'static,
        W5500,
        ExclusiveDevice<Spi<'static, SPI0, Async>, Output<'static>, embassy_time::Delay>,
        Input<'static>,
        Output<'static>,
    >,
) -> ! {
    runner.run().await
}

// ── Net task ───────────────────────────────────────────────────────────────────

#[embassy_executor::task]
async fn net_task(mut runner: embassy_net::Runner<'static, Device<'static>>) -> ! {
    runner.run().await
}

// ── Main ───────────────────────────────────────────────────────────────────────

#[embassy_executor::main]
async fn main(spawner: Spawner) {
    // 1. Init embassy-rp HAL
    let p = embassy_rp::init(Default::default());
    let mut rng = RoscRng;

    // 2. Factory reset check — hold PIN_26 low at boot to trigger reset
    let factory_reset_pin = Input::new(p.PIN_26, Pull::Up);
    let factory_reset_requested = factory_reset_pin.is_low();
    drop(factory_reset_pin);

    // 3. Init flash → ekv database (async flash using DMA_CH2)
    let flash: Flash<'static, _, FlashAsync, FLASH_SIZE> =
        Flash::new(p.FLASH, p.DMA_CH2, Irqs);
    let random_seed_u32 = rng.next_u32();
    let db = init_database(flash, random_seed_u32).await;

    if factory_reset_requested {
        warn!("Factory reset triggered! Formatting ekv and seeding defaults.");
        db.format().await.ok();
        storage::seed_defaults(db).await;
    }

    // Register DB with the web layer (all handlers access it via crate::web::db())
    web::init_db(db);

    // 4. GPIO outputs for 8 relay outputs (PIN_0 – PIN_7)
    let pin0 = Output::new(p.PIN_0, Level::Low);
    let pin1 = Output::new(p.PIN_1, Level::Low);
    let pin2 = Output::new(p.PIN_2, Level::Low);
    let pin3 = Output::new(p.PIN_3, Level::Low);
    let pin4 = Output::new(p.PIN_4, Level::Low);
    let pin5 = Output::new(p.PIN_5, Level::Low);
    let pin6 = Output::new(p.PIN_6, Level::Low);
    let pin7 = Output::new(p.PIN_7, Level::Low);

    // 6. Spawn GPIO task
    spawner.spawn(gpio_task(pin0, pin1, pin2, pin3, pin4, pin5, pin6, pin7).unwrap());

    // 7. Init W5500 Ethernet (SPI0, DMA_CH0 TX, DMA_CH1 RX)
    let mut spi_cfg = SpiConfig::default();
    spi_cfg.frequency = SPI_FREQ_HZ;
    let spi = Spi::new(
        p.SPI0,
        p.PIN_18, // CLK
        p.PIN_19, // MOSI
        p.PIN_16, // MISO
        p.DMA_CH0,
        p.DMA_CH1,
        Irqs,
        spi_cfg,
    );
    let cs = Output::new(p.PIN_17, Level::High);
    let w5500_int = Input::new(p.PIN_21, Pull::Up);
    let w5500_reset = Output::new(p.PIN_20, Level::High);

    let mac_addr = [0x02, 0x00, 0x00, 0x00, 0x00, 0x01];
    static W5500_STATE: StaticCell<embassy_net_wiznet::State<8, 8>> = StaticCell::new();
    let state = W5500_STATE.init(embassy_net_wiznet::State::<8, 8>::new());
    let (device, runner) = embassy_net_wiznet::new(
        mac_addr,
        state,
        ExclusiveDevice::new(spi, cs, embassy_time::Delay),
        w5500_int,
        w5500_reset,
    )
    .await
    .unwrap();

    // 8. Spawn Ethernet + network tasks
    spawner.spawn(ethernet_task(runner).unwrap());

    let seed = rng.next_u64();
    static STACK_RESOURCES: StaticCell<StackResources<{ WEB_TASK_POOL_SIZE + 2 }>> =
        StaticCell::new();
    let (stack, net_runner) = embassy_net::new(
        device,
        embassy_net::Config::dhcpv4(Default::default()),
        STACK_RESOURCES.init(StackResources::new()),
        seed,
    );

    spawner.spawn(net_task(net_runner).unwrap());

    // 9. Watchdog — start after spawning tasks
    let mut watchdog = Watchdog::new(p.WATCHDOG);
    watchdog.start(Duration::from_secs(8));

    // 10. Wait for DHCP
    info!("Waiting for DHCP...");
    loop {
        if stack.config_v4().is_some() {
            break;
        }
        watchdog.feed(Duration::from_secs(8));
        Timer::after_millis(200).await;
    }
    let ip = stack.config_v4().unwrap().address;
    info!("IP address: {}", ip);

    // 11. Spawn sensor task (ADC + internal temperature sensor)
    let adc = Adc::new(p.ADC, Irqs, AdcConfig::default());
    let ts_channel = Channel::new_temp_sensor(p.ADC_TEMP_SENSOR);
    spawner.spawn(sensor_task(adc, ts_channel).unwrap());

    // 12. Build picoserve app (stateless — auth + DB access via module-level statics)
    static APP: StaticCell<picoserve::AppRouter<App>> = StaticCell::new();
    let app = APP.init(App.build_app());

    // 13. Spawn 4× web_task
    for id in 0..WEB_TASK_POOL_SIZE {
        spawner.spawn(web_task(id, stack, app, &CONFIG).unwrap());
    }

    // 14. LED blink ready signal
    let mut led = Output::new(p.PIN_25, Level::Low);
    for _ in 0..3 {
        led.set_high();
        Timer::after_millis(100).await;
        led.set_low();
        Timer::after_millis(100).await;
        watchdog.feed(Duration::from_secs(8));
    }
    led.set_high();

    info!("PDU ready on http://{}", ip);

    // Main loop — feed watchdog
    loop {
        watchdog.feed(Duration::from_secs(8));
        Timer::after_secs(4).await;
    }
}
