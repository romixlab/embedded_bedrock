// embedded_bedrock TPL-24: the embassy-boot updater and the settings store each take the flash
#[embassy_executor::main]
async fn main(_spawner: Spawner) {
    let p = embassy_stm32::init(Default::default());
    let flash = Mutex::new(RefCell::new(Flash::new_blocking(p.FLASH)));
    let config = FirmwareUpdaterConfig::from_linkerfile_blocking(&flash, &flash);
    settings::init();
}

mod settings {
    pub fn init() {
        let flash = Flash::new_blocking(unsafe { embassy_stm32::peripherals::FLASH::steal() });
    }
}
