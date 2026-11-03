// report_template, embedded_bedrock TPL-48: a button with the pad pull-down reads pressed for good on RP2350 A2
#[embassy_executor::main]
async fn main(_spawner: Spawner) {
    let p = embassy_rp::init(Default::default());
    let button = Input::new(p.PIN_15, Pull::Down);
    let other = Input::new(p.PIN_16, Pull::Up);
    pac::PADS_BANK0.gpio(17).modify(|w| w.set_pde(true));
}
