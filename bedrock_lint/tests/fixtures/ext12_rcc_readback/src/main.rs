// donguru_template main.rs: the clock enable is read back before the peripheral's first access
fn variant() -> u32 {
    pac::RCC.apbenr1().modify(|w| w.set_dbgen(true));
    pac::DBGMCU.idcode().read().0
}

fn backup_registers() {
    let rcc = pac::RCC;
    rcc.apbenr1().modify(|w| w.set_pwren(true));
    let _ = rcc.apbenr1().read(); // the enable went through
    pac::PWR.cr1().modify(|w| w.set_dbp(true));
}
