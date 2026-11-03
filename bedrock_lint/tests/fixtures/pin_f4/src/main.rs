#![no_std]
#![no_main]

// An F4: pins reset to floating inputs, and nothing sets the unused ones to analog

#[embassy_executor::main]
async fn main(_spawner: Spawner) {
    let p = embassy_stm32::init(Default::default());
    let mut led = Output::new(p.PC13, Level::High, Speed::Low);
    loop {
        led.toggle();
        Timer::after_millis(500).await;
    }
}
