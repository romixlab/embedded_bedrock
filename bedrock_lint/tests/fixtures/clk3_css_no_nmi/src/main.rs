// CSS on, but the NMI it raises goes to DefaultHandler, which returns with CSSF set
#[embassy_executor::main]
async fn main(_spawner: Spawner) {
    let mut config = embassy_stm32::Config::default();
    config.rcc.hse = Some(Hse { freq: Hertz(8_000_000), mode: HseMode::Oscillator });
    let p = embassy_stm32::init(config);
    pac::RCC.cr().modify(|w| w.set_csson(true));
}
