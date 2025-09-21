#![no_std]
#![no_main]
{% if use_nightly %}
#![feature(impl_trait_in_assoc_type)]
{% endif %}

mod init;
mod build_info;
{% if have_init_ram -%}
mod init_ram;
{% endif -%}

use defmt::*;
use defmt_rtt as _;
use embassy_executor::Spawner;
use embassy_stm32::Config;
use embassy_stm32::gpio::{Level, Output, Speed};
use embassy_time::Timer;
use panic_probe as _;
use cortex_m_rt::exception;
{% if use_counters and use_bkp_counters == false -%}
use cnt_macro::cnt_if;
{% endif -%}
{% if use_counters and use_bkp_counters -%}
use cnt_macro::{cnt_if, bkp_cnt_if};
{% endif -%}
{% if supply_config != "" -%}
use embassy_stm32::rcc::SupplyConfig;
{% endif -%}
{% if smps_supply_voltage != "" -%}
use embassy_stm32::rcc::SMPSSupplyVoltage;
{% endif -%}
{% if use_bootloader -%}
use embassy_boot_stm32::{AlignedBuffer, BlockingFirmwareUpdater, FirmwareUpdaterConfig};
use embassy_stm32::flash::{Flash, WRITE_SIZE};
use embassy_sync::blocking_mutex::Mutex;
// use embedded_storage::nor_flash::NorFlash;
use core::cell::RefCell;
{% endif %}

#[embassy_executor::main]
async fn main(_spawner: Spawner) {
    {% if have_init_ram -%}
    init_ram::init_ram();
    {% endif -%}
    info!("{{project-name}} starting...");
    {% if supply_config != "" -%}
    let mut config = Config::default();
    {% if smps_supply_voltage == "" -%}
    config.rcc.supply_config = SupplyConfig::{{ supply_config }};
    {% else -%}
    config.rcc.supply_config = SupplyConfig::{{ supply_config }}(SMPSSupplyVoltage::{{ smps_supply_voltage }});
    {% endif -%}
    let p = embassy_stm32::init(config);
    {% else %}
    let p = embassy_stm32::init(Config::default());
    {% endif -%}
    init::init();
    {% if use_rtc == false -%}
    init::reset_bkp_domain();
    {% endif -%}
    info!("RCC and RAM init done");
    _ = core::hint::black_box(build_info::compact()); // ensure compact build info is in FLASH
    _ = core::hint::black_box(build_info::full()); // ensure full build info is in ELF

    {% if chip contains "stm32h7" %}
    let mut cp = cortex_m::Peripherals::take().unwrap();
    cp.SCB.enable_icache();
    // Enable D-Cache only after verifying that no coherency issues will arise, e.g., when using DMAs
    // DMAs write/read to/from SRAM while cache continues to hold old data, can use cache invalidate to solve this
    // cp.SCB.enable_dcache(&mut cp.CPUID);
    {% endif -%}

    {% if use_bootloader %}
    let flash = Flash::new_blocking(p.FLASH);
    let flash = Mutex::new(RefCell::new(flash));
    let config = FirmwareUpdaterConfig::from_linkerfile_blocking(&flash, &flash);
    let mut magic = AlignedBuffer([0; WRITE_SIZE]);
    let mut updater = BlockingFirmwareUpdater::new(config, &mut magic.0);
    info!("updater state: {}", updater.get_state());
    // TODO: consider calling mark_booted after ensuring a fw is actually working (e.g., run some tests)
    updater.mark_booted().unwrap();
    {% endif -%}

    let mut led = Output::new(p.PB14, Level::Low, Speed::Low);

    info!("Init done");
    loop {
        info!("LED ON");
        led.set_high();
        Timer::after_millis(2000).await;

        info!("LED OFF");
        led.set_low();
        Timer::after_millis(2000).await;
    }
}

#[exception]
unsafe fn DefaultHandler(irqn: i16) {
    {% if use_counters -%}
    cnt_if!(true, unhandled_exceptions: u32);
    {% endif -%}
    {% if use_bkp_counters -%}
    bkp_cnt_if!(true, unhandled_exceptions_total: u32);
    {% endif -%}
    error!("Unhandled exception (IRQn = {})", irqn);
}

#[exception]
unsafe fn HardFault(ef: &cortex_m_rt::ExceptionFrame) -> ! {
    {% if use_bkp_counters -%}
    bkp_cnt_if!(true, hard_faults: u32);
    {% endif -%}
    error!("HardFault {}", defmt::Debug2Format(ef));

    loop {}
}

//NonMaskableInt (CSS?)
// NOTE that at this point we don't check if the exception is available on the target (e.g.
// MemoryManagement is not available on Cortex-M0)
// "MemoryManagement" | "BusFault" | "UsageFault" | "SecureFault" | "SVCall"
// | "DebugMonitor" | "PendSV" | "SysTick" => {
