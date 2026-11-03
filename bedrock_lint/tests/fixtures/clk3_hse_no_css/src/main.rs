// embedded_bedrock ROB-5, vb135 clocks.rs: the HSE feeds the PLL, nothing watches it
#[embassy_executor::main]
async fn main(_spawner: Spawner) {
    let mut config = embassy_stm32::Config::default();
    config.rcc.hse = Some(Hse {
        freq: Hertz(25_000_000),
        mode: HseMode::Oscillator,
    });
    config.rcc.pll1 = Some(Pll { source: PllSource::HSE, ..PLL });
    let p = embassy_stm32::init(config);
}
