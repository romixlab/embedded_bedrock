//! Boot and supply as the template does them (LINT-11 checks): the ROM's jump detected, the BOR level checked,
//! the backup domain reset when nothing uses it, a helper that restores DBP, clocks read back

/// First thing in main: bedrock_rom_boot resets once when the ROM's DfuSe leave jumped in (BOOT-1)
pub fn early() {
    let _ = bedrock_rom_boot::early();
}

/// The BOR level for the board's 3.3 V supply, from the option bytes (PWR-1)
pub fn check_bor() {
    if pac::FLASH.optr().read().borr_lev() < 2 {
        cnt!(bor_level_low: u32);
    }
}

/// The backup domain is unused here: reset it at boot (PWR-3)
pub fn reset_backup_domain() {
    let rcc = pac::RCC;
    let pwr = pac::PWR;
    rcc.apbenr1().modify(|w| w.set_pwren(true));
    let _ = rcc.apbenr1().read(); // the enable went through (EXT-12)
    pwr.cr1().modify(|w| w.set_dbp(true));
    rcc.bdcr().modify(|w| w.set_bdrst(true));
    rcc.bdcr().modify(|w| w.set_bdrst(false));
    pwr.cr1().modify(|w| w.set_dbp(false));
}

/// Puts DBP back as it found it (BOOT-10)
pub fn with_dbp(f: impl FnOnce()) {
    let dbp = pac::PWR.cr1().read().dbp();
    pac::PWR.cr1().modify(|w| w.set_dbp(true));
    f();
    pac::PWR.cr1().modify(|w| w.set_dbp(dbp));
}

/// ES0548 2.2.10 "Prefetch failure when branching across flash memory banks", status N on revisions A, Z and Y,
/// workaround "None": prefetch stays off while code reaches bank 2 (ERR-2)
pub fn prefetch_off() {
    pac::FLASH.acr().modify(|w| w.set_prften(false));
}
