//! Flash as the template does it (LINT-11 checks): one ECC-safe driver with its NMI hook, the image's CRC checked

/// One flash driver, behind ww_ecc_flash (FLS-1, FLS-2)
pub fn flash(p: FLASH) -> EccSafeFlash<Flash<'static, Blocking>> {
    unsafe { ww_ecc_flash::EccSafeFlash::new(Flash::new_blocking(p), 0x0800_0000) }
}

#[exception]
unsafe fn NonMaskableInt() {
    if ww_ecc_flash::on_nmi() {
        return;
    }
    cnt!(nmi: u32);
}

/// The image's CRC against the one the build stored, at boot and every hour (FLS-4)
#[embassy_executor::task]
pub async fn image_crc_task() {
    loop {
        if image_crc() != stored_image_crc() {
            cnt!(image_crc_mismatch: u32, error);
        }
        Timer::after_secs(3600).await;
    }
}
