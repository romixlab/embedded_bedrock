// typec_dongle API-19, embassy#7235: settings read raw from flash on an ECC part; a write torn by a power cut is
// an NMI on every boot
#[embassy_executor::main]
async fn main(_spawner: Spawner) {
    let p = embassy_stm32::init(Default::default());
    let flash = Flash::new_blocking(p.FLASH);
    let settings = ww_settings::Store::new(BlockingAsync::new(flash));
}
