//! embassy-stm32 glue: the driver on embassy's `Peri<USB>`, its interrupt binding and RCC enable. Nothing else of
//! embassy-stm32's USB driver is used.
//!
//! ```ignore
//! bind_interrupts!(struct Irqs {
//!     // the G0 shares its vector with UCPD: each handler looks at its own flags
//!     USB_UCPD1_2 => bedrock_usb::v4::embassy::InterruptHandler<peripherals::USB>, ucpd::InterruptHandler<peripherals::UCPD2>;
//! });
//! bedrock_usb::state!(static USB_STATE: Desc);
//! let usb = bedrock_usb::v4::embassy::new::<Desc, _, _>(p.USB, Irqs, p.PA12, p.PA11, &USB_STATE, 64_000_000);
//! usb.connect();
//! ```

use core::marker::PhantomData;

use embassy_stm32::Peri;
use embassy_stm32::interrupt::typelevel::{Binding, Handler, Interrupt};
use embassy_stm32::usb::{DmPin, DpPin, Instance};

use super::{Driver, Mmio, State};
use crate::Descriptors;

/// The USB interrupt handler for `bind_interrupts!`: calls [super::on_interrupt].
pub struct InterruptHandler<T: Instance> {
    _p: PhantomData<T>,
}

impl<T: Instance> Handler<T::Interrupt> for InterruptHandler<T> {
    unsafe fn on_interrupt() {
        super::on_interrupt()
    }
}

/// Takes the peripheral, enables its clock, registers and starts the driver ([Driver::init]) and unmasks its
/// interrupt. The binding is the proof that [InterruptHandler] is on the vector, as with embassy's own driver;
/// the pins are taken so nothing else uses them (the G0's DP and DM need no alternate function). `hclk_hz` is the
/// core clock the tSTARTUP and ES0548 2.16.1 waits count in. The 48 MHz USB kernel clock (HSI48 with CRS, or the
/// PLL) is the firmware's RCC configuration. The device stays off the bus until [Driver::connect].
pub fn new<D: Descriptors, T: Instance, const N: usize>(
    _usb: Peri<'static, T>,
    _irq: impl Binding<T::Interrupt, InterruptHandler<T>> + 'static,
    _dp: Peri<'static, impl DpPin<T>>,
    _dm: Peri<'static, impl DmPin<T>>,
    state: &'static State<N>,
    hclk_hz: u32,
) -> Driver<D, Mmio> {
    embassy_stm32::rcc::enable_and_reset::<T>();
    // Safety: the `Peri` is ours, so are its registers and USB SRAM
    let regs = unsafe {
        Mmio::new(
            embassy_stm32::pac::USB.as_ptr() as usize,
            embassy_stm32::pac::USBRAM.as_ptr() as usize,
            hclk_hz,
        )
    };
    let usb = Driver::new(regs, state);
    usb.register();
    usb.init();
    T::Interrupt::unpend();
    // Safety: the handler is bound (the Binding) and the driver it serves is registered
    unsafe { T::Interrupt::enable() };
    usb
}
