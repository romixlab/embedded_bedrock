#![no_std]
#![no_main]

mod build_info;

use core::cell::RefCell;

use cortex_m_rt::{entry, exception};
#[cfg(feature = "defmt")]
use defmt_rtt as _;
use embassy_boot_stm32::*;
use embassy_stm32::Config;
use embassy_stm32::flash::{Flash, BANK1_REGION};
use embassy_sync::blocking_mutex::Mutex;
{% if supply_config != "" %}
use embassy_stm32::rcc::SupplyConfig;
{% endif -%}
{% if smps_supply_voltage != "" %}
use embassy_stm32::rcc::SMPSSupplyVoltage;
{% endif %}

#[entry]
fn main() -> ! {
    {% if supply_config != "" -%}
    let mut config = Config::default();
    {% if smps_supply_voltage == "" -%}
    config.rcc.supply_config = SupplyConfig::{{ supply_config }};
    {% else -%}
    config.rcc.supply_config = SupplyConfig::{{ supply_config }}(SMPSSupplyVoltage::{{ smps_supply_voltage }});
    {% endif -%}
    let p = embassy_stm32::init(config);
    {% else %}
    let p = embassy_stm32::init(Default::default());
    {% endif -%}
    _ = core::hint::black_box(build_info::compact()); // ensure compact build info is in FLASH
    _ = core::hint::black_box(build_info::full()); // ensure full build info is in ELF

    // Uncomment this if you are debugging the bootloader with debugger/RTT attached,
    // as it prevents a hard fault when accessing flash 'too early' after boot.
    /*
        for i in 0..10000000 {
            cortex_m::asm::nop();
        }
    */

    let layout = Flash::new_blocking(p.FLASH).into_blocking_regions();
    let flash = Mutex::new(RefCell::new(layout.bank1_region));

    let config = BootLoaderConfig::from_linkerfile_blocking(&flash, &flash, &flash);
    let active_offset = config.active.offset();
    let bl = BootLoader::prepare::<_, _, _, 2048>(config);

    unsafe { bl.load(BANK1_REGION.base + active_offset) }
}

// fn blink(led: &mut Output, n: u32) {
//     for _ in 0..n {
//         led.set_low();
//         cortex_m::asm::delay(24_000_000);
//         led.set_high();
//         cortex_m::asm::delay(24_000_000);
//     }
//     cortex_m::asm::delay(64_000_000);
// }

#[unsafe(no_mangle)]
#[cfg_attr(target_os = "none", unsafe(link_section = ".HardFault.user"))]
unsafe extern "C" fn HardFault() {
    cortex_m::peripheral::SCB::sys_reset();
}

#[exception]
unsafe fn DefaultHandler(irqn: i16) -> ! {
    // const SCB_ICSR: *const u32 = 0xE000_ED04 as *const u32;
    // let irqn = unsafe { core::ptr::read_volatile(SCB_ICSR) } as u8 as i16 - 16;

    panic!("DefaultHandler #{:?}", irqn);
}

#[panic_handler]
fn panic(_info: &core::panic::PanicInfo) -> ! {
    cortex_m::asm::udf();
}
