#![no_std]
#![no_main]
#![feature(impl_trait_in_assoc_type)]

use defmt::*;
use defmt_rtt as _;
use embassy_stm32::gpio::{Level, Output, Speed};
use embassy_time::Timer;
use panic_probe as _;
{% if use_counters and use_bkp_counters == false -%}
use cnt_macro::cnt_if;
{% endif %}

#[embassy_executor::main]
async fn main(_spawner: embassy_executor::Spawner) {
    {% if supply_config != "" -%}
    let mut config = embassy_stm32::Config::default();
    {% if smps_supply_voltage == "" -%}
    config.rcc.supply_config = SupplyConfig::{{ supply_config }};
    {% else -%}
    config.rcc.supply_config = SupplyConfig::{{ supply_config }}(SMPSSupplyVoltage::{{ smps_supply_voltage }});
    {% endif -%}
    let p = embassy_stm32::init(config);
    {% else %}
    let p = embassy_stm32::init(embassy_stm32::Config::default());
    {% endif -%}
    info!("Hello World!");

    let mut led = Output::new(p.PB14, Level::Low, Speed::Low);

    loop {
        info!("LED ON");
        led.set_high();
        Timer::after_millis(2000).await;

        info!("LED OFF");
        led.set_low();
        Timer::after_millis(2000).await;

        {% if use_counters -%}
        cnt_if!(true, blinks_count_after_reset: u32);
        {% endif %}
    }
}
