#![no_std]
#![no_main]

// Fault handling done wrong: typec_dongle FW-48 (cnt! and error! panicked in DefaultHandler), usb_io_fw PLAT-7
// (HardFault spins), embedded_bedrock TPL-64 (USB firmware that never marks the boot healthy)

#[embassy_executor::main]
async fn main(spawner: Spawner) {
    let p = embassy_stm32::init(Default::default());
    let mut wdg = IndependentWatchdog::new(p.IWDG, 1_000_000);
    wdg.unleash();
    spawner.spawn(feeder(wdg).unwrap());
    spawner.spawn(healthy().unwrap());
    let usb = embassy_usb::Builder::new(driver, config, &mut [], &mut [], &mut [], &mut []);
}

#[embassy_executor::task]
async fn feeder(mut wdg: IndependentWatchdog<'static>) {
    loop {
        wdg.pet();
        Timer::after_millis(500).await;
    }
}

#[embassy_executor::task]
async fn healthy() {
    bedrock_rom_boot::healthy_watch(Duration::from_secs(10)).await;
}

#[interrupt]
fn TIM2() {
    WDG.lock(|w| w.borrow_mut().as_mut().unwrap().pet());
}

#[exception]
unsafe fn DefaultHandler(irqn: i16) {
    cnt!(unhandled_exceptions: u32, warn);
    error!("unhandled exception, IRQn = {}", irqn);
}

#[exception]
unsafe fn HardFault(ef: &cortex_m_rt::ExceptionFrame) -> ! {
    error!("HardFault {}", defmt::Debug2Format(ef));
    let exc_return = bedrock_crash::exc_return!();
    let _ = exc_return;
    loop {}
}
