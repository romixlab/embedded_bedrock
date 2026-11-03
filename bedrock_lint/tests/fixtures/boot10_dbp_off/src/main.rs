// vb135 BOOT-5/6: the helper for the boot word cleared DBP and RTCEN behind it, so every boot read "unhealthy"
fn write_boot_word(w: u32) {
    pac::PWR.cr1().modify(|r| r.set_dbp(true));
    pac::RCC.bdcr().modify(|r| r.set_rtcen(true));
    pac::RTC.bkpr(0).write_value(w);
    pac::RCC.bdcr().modify(|r| r.set_rtcen(false));
    pac::PWR.cr1().modify(|r| r.set_dbp(false));
}

// right: what was there is put back
fn write_boot_word_restoring(w: u32) {
    let dbp = pac::PWR.cr1().read().dbp();
    pac::PWR.cr1().modify(|r| r.set_dbp(true));
    pac::RTC.bkpr(0).write_value(w);
    pac::PWR.cr1().modify(|r| r.set_dbp(dbp));
}
