#![no_std]
#![no_main]

// Pins done wrong: donguru_template (a driver dropped at once), usb_io_fw PLAT-9 (USB before the safe state),
// usb_io_fw IF-16 (two owners), P2424 (pin map by feature), typec_dongle FW-22 (a strap read under the dead-battery
// pull-down)

#[embassy_executor::main]
async fn main(spawner: Spawner) {
    let p = embassy_stm32::init(Default::default());
    let usb = embassy_usb::Builder::new(driver, config, &mut [], &mut [], &mut [], &mut []);
    let _ = Output::new(p.PA5, Level::High, Speed::Low);
    Output::new(p.PB0, Level::Low, Speed::Low);
    let enable = Output::new(p.PC6, Level::Low, Speed::Low);
    #[cfg(feature = "rev_b")]
    let led = Output::new(p.PB7, Level::Low, Speed::Low);
    spawner.spawn(diag(p.PA9).unwrap());
    let variant = Input::new(p.PA10, Pull::Up).is_high();
    pac::SYSCFG.cfgr1().modify(|w| w.set_ucpd1_strobe(true));
    let _ = (usb, variant);
}

#[embassy_executor::task]
async fn diag(pin: Peri<'static, PA9>) {
    let stolen = unsafe { PA9::steal() };
    pac::SYSCFG.cfgr1().modify(|w| w.set_ucpd1_strobe(false));
}
