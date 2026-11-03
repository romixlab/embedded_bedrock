#![no_std]
#![no_main]

// usb_io_fw IF-2: an I2C pin switched to SIO and back while the block was enabled

fn wiggle(pin: u8) {
    pac::IO_BANK0.gpio(pin as usize).ctrl().write(|w| w.set_funcsel(FUNC_SIO));
    pac::IO_BANK0.gpio(pin as usize).ctrl().write(|w| w.set_funcsel(FUNC_I2C));
}

fn hold_low(pin: u8) {
    pac::IO_BANK0.gpio(pin as usize).ctrl().write(|w| {
        w.set_funcsel(FUNC_I2C);
        w.set_oeover(Oeover::Enable);
    });
}

fn probe() -> bool {
    let sda = Flex::new(unsafe { PIN_4::steal() });
    sda.is_high()
}
