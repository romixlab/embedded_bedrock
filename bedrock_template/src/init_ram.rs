pub(crate) fn init_ram() {
    {% if chip contains "stm32g0" and use_bkp_counters -%}
    enable_tamp_bkp_regs()
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