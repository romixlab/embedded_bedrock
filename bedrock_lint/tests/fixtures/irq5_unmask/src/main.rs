mod interrupt {
    pub struct Irq;
    impl Irq {
        pub fn enable(&self) {}
    }
    pub static I2C0_IRQ: Irq = Irq;
    pub static I2C1_IRQ: Irq = Irq;
}

enum Interrupt {
    USB,
    SPI1,
}

struct NVIC;
impl NVIC {
    fn unmask(_i: Interrupt) {}
}

bind_interrupts!(struct Irqs {
    USB => usb::InterruptHandler<USB>;
});

#[interrupt]
fn I2C0_IRQ() {}

fn setup_i2c0() {
    unsafe { interrupt::I2C0_IRQ.enable() };
}

fn setup_i2c1() {
    unsafe { interrupt::I2C1_IRQ.enable() };
}

fn setup_usb() {
    unsafe { NVIC::unmask(Interrupt::USB) };
}

fn setup_spi1() {
    unsafe { NVIC::unmask(Interrupt::SPI1) };
}

struct Can;
struct Reg;
impl Reg {
    fn write_value(&self, _v: u32) {}
}
impl Can {
    fn ils(&self) -> Reg {
        Reg
    }
}

fn route_can_lines(can: &Can) {
    can.ils().write_value(1);
}
