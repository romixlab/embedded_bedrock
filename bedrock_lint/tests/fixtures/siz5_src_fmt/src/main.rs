// P2534 fw-size 2026-10-19: Result::unwrap, Debug2Format and a formatting panic handler pull core::fmt in
#[embassy_executor::main]
async fn main(spawner: Spawner) {
    let p = embassy_stm32::init(Default::default());
    let n: u8 = 300u16.try_into().unwrap();
    spawner.spawn(blink().unwrap());
    spawner.spawn(defmt::unwrap!(blink()));
    let first = [1u8].first().unwrap();
    defmt::info!("{}", defmt::Debug2Format(&p));
}

#[embassy_executor::task]
async fn blink() {}

#[panic_handler]
fn panic(info: &core::panic::PanicInfo) -> ! {
    defmt::error!("{}", defmt::Display2Format(info));
    loop {}
}
