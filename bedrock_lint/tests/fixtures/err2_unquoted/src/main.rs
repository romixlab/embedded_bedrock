// tpm firmware pack: errata pokes say which erratum they work around
fn init() {
    // errata workaround: the prefetch has to be off on this part
    pac::FLASH.acr().modify(|w| w.set_prften(false));
    // ES0548 2.2.10: on a dual-bank part prefetch can fail on a branch into the other bank
    embassy_stm32::pac::FLASH
        .acr()
        .modify(|w| w.set_prften(false));
    // ES0548 2.2.10 "Prefetch failure when branching across flash memory banks", status N on revisions A, Z and Y,
    // workaround "None" (prefetch off)
    pac::FLASH.acr().modify(|w| w.set_prften(false));
    // instruction cache on, nothing to quote
    pac::FLASH.acr().modify(|w| w.set_icen(true));
}
