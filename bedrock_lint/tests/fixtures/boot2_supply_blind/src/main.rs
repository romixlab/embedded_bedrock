// vb135 BOOT-8, embedded_bedrock TPL-69: after the ROM bootloader (before V9.3) PWR_CR3 is locked and this write
// is ignored
#[embassy_executor::main]
async fn main(_spawner: Spawner) {
    let mut config = embassy_stm32::Config::default();
    config.rcc.supply_config = SupplyConfig::DirectSMPS;
    let p = embassy_stm32::init(config);
}

fn bootloader_config() -> embassy_stm32::Config {
    let mut c = embassy_stm32::Config::default();
    c.rcc = Rcc {
        supply_config: embassy_stm32::rcc::SupplyConfig::SMPSLDO(SMPSSupplyVoltage::V1_8),
        ..Default::default()
    };
    c
}
