{% if use_bkp_counters -%}
/// Enable backup counters
{% endif -%}
pub(crate) fn init() {
    {% if chip contains "stm32g0" and use_bkp_counters -%}
    enable_tamp_bkp_regs();
    {% endif -%}
    {% if chip contains "stm32h7" and use_bkp_counters -%}
    enable_rcc_bkp_regs();
    {% endif -%}
}

{% if chip contains "stm32g0" and use_bkp_counters -%}
fn enable_tamp_bkp_regs() {
    use embassy_stm32::pac::rcc::vals::Rtcsel;
    
    let rcc = embassy_stm32::pac::RCC;
    if !rcc.csr().read().lsion() {
        // Possible to use other RTC clock sources, without RTC enabled TAMP registers are not writeable
        // ref. manual is not very clear on it
        defmt::panic!("LSI must be on for TAMP BKP register access");
    }
    let pwr = embassy_stm32::pac::PWR;
    pwr.cr1().modify(|w| w.set_dbp(true));
    rcc.apbenr1().modify(|w| w.set_rtcapben(true));
    rcc.bdcr().modify(|w| {
        w.set_rtcsel(Rtcsel::LSI);
        w.set_rtcen(true);
    });
}
{% endif -%}

{% if chip contains "stm32h7" and use_bkp_counters -%}
// TODO: This does not really work on H7, RCC backup regs are corrupted and same goes for Backup SRAM which is not preserved
fn enable_rcc_bkp_regs() {
    use embassy_stm32::pac::rcc::vals::Rtcsel;
    let rcc = embassy_stm32::pac::RCC;
    let pwr = embassy_stm32::pac::PWR;

    rcc.ahb4enr().modify(|w| w.set_bkpsramen(true));
    rcc.c1_ahb4enr().modify(|w| w.set_bkpsramen(true));

    if !rcc.csr().read().lsion() {
        // Possible to use other RTC clock sources, without RTC enabled TAMP registers are not writeable
        // ref. manual is not very clear on it
        defmt::panic!("LSI must be on for TAMP BKP register access");
    }

    pwr.cr2().modify(|w| w.set_bren(true));
    while pwr.cr2().read().brrdy() == false {
        cortex_m::asm::nop();
    }

    pwr.cr1().modify(|w| w.set_dbp(true));
    info!("dbp={}", pwr.cr1().read().dbp());
    rcc.apb4enr().modify(|w| w.set_rtcapben(true));
    rcc.bdcr().modify(|w| w.set_bdrst(true));
    info!("bdrst after setting: {}", rcc.bdcr().read().bdrst());
    rcc.bdcr().modify(|w| w.set_bdrst(false));
    rcc.bdcr().modify(|w| {
        w.set_rtcsel(Rtcsel::LSI);
        w.set_rtcen(true);
    });
    cortex_m::asm::dsb();
}
{% endif -%}

{% if use_rtc == false -%}
/// Corrupted content of the RTC domain due to a missed power-on reset after this domain supply voltage drop.
/// Leads to hard to debug gotchas (LSE enables by itself, PC13 / PC14 / PC15 and others set to output).
/// The solution is to reset the backup domain when RTC is not used.
/// See: http://efton.sk/STM32/gotcha/g133.html and http://efton.sk/STM32/gotcha/g62.html
pub(crate) fn reset_bkp_domain() {
    let rcc = embassy_stm32::pac::RCC;
    let pwr = embassy_stm32::pac::PWR;

    {%- if rcc_have_pwren %}
    rcc.apbenr1().modify(|w| w.set_pwren(true));
    let _ = rcc.apbenr1().read();
    {% endif -%}

    pwr.cr1().modify(|w| w.set_dbp(true));
    let mut cr1 = pwr.cr1().read(); // to ensure the write went through the synchronizer

    rcc.bdcr().modify(|w| w.set_bdrst(true));
    rcc.bdcr().modify(|w| w.set_bdrst(false));

    cr1.set_dbp(false);
    pwr.cr1().write_value(cr1);
}
{% endif -%}
