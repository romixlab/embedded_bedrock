// embedded_bedrock TPL-13: F411 has no HSI48, and the clock tree was left as a TODO
#[embassy_executor::main]
async fn main(spawner: Spawner) {
    let config = embassy_stm32::Config::default();
    // TODO: a 48 MHz USB clock
    let p = embassy_stm32::init(config);
    let driver = embassy_stm32::usb::Driver::new_fs(p.USB_OTG_FS, Irqs, p.PA12, p.PA11, &mut EP_OUT, Default::default());
}
