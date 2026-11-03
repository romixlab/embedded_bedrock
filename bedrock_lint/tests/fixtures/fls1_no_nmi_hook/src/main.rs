// typec_dongle API-19: the flash is wrapped, but the G0's NMI never reaches ww_ecc_flash
#[embassy_executor::main]
async fn main(_spawner: Spawner) {
    let p = embassy_stm32::init(Default::default());
    let flash = unsafe { ww_ecc_flash::EccSafeFlash::new(Flash::new_blocking(p.FLASH), 0x0800_0000) };
}

#[exception]
unsafe fn NonMaskableInt() {
    cnt!(nmi: u32);
}
