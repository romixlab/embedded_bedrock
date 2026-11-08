//! embassy-stm32 glue for usb_v3 (L4, G4, WB): the driver on embassy's `Peri<USB>`, its interrupt binding, RCC
//! enable, VDDUSB and the pins. Nothing else of embassy-stm32's USB driver is used.
//!
//! ```ignore
//! bind_interrupts!(struct Irqs {
//!     USB_LP => bedrock_usb::v3::embassy::InterruptHandler<peripherals::USB>; // USB on the L4
//! });
//! bedrock_usb::state!(static USB_STATE: Desc);
//! let usb = bedrock_usb::v3::embassy::new::<Desc, _, _>(
//!     p.USB, Irqs, p.PA12, p.PA11, &USB_STATE, Family::G4, 170_000_000,
//! );
//! usb.connect();
//! ```

use core::marker::PhantomData;

use embassy_stm32::Peri;
use embassy_stm32::gpio::{AfType, Flex, OutputType, Speed};
use embassy_stm32::interrupt::typelevel::{Binding, Handler, Interrupt};
use embassy_stm32::usb::{DmPin, DpPin, Instance};

use super::{Driver, Family, Mmio, State};
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

/// A DP or DM pin in the mode its chip wants. Where the pin lists USB as an alternate function (the L4 and WB:
/// AF10 in stm32-metapac's data), it is switched to it, as embassy-stm32's own driver does; where USB is an additional function (the G4:
/// "USB_DM" / "USB_DP" in the additional-functions column of DS12288 Rev 6 Table 12), it stays analog, as
/// "enabling AF will break USB" (embassy-stm32 build.rs). embassy's `af_num()` is 0 for a pin without one; no
/// chip here has USB on AF0. The pin stays ours: the `Flex` is forgotten, so its drop doesn't disconnect it.
fn usb_pin(pin: Peri<'static, impl embassy_stm32::gpio::Pin>, af: u8) {
    if af != 0 {
        let mut flex = Flex::new(pin);
        flex.set_as_af_unchecked(af, AfType::output(OutputType::PushPull, Speed::VeryHigh));
        core::mem::forget(flex);
    }
}

/// Takes the peripheral, validates VDDUSB on the L4 and WB ([Family::validate_vddusb]), enables the USB clock,
/// sets the pins, registers and starts the driver ([Driver::init]) and unmasks its interrupt. The binding is the
/// proof that [InterruptHandler] is on the vector, as with embassy's own driver. `hclk_hz` is the core clock the
/// tSTARTUP and OUT-packet waits count in. The 48 MHz USB clock (HSI48 with CRS, or a PLL) and an APB clock of at
/// least 10 MHz (the errata in [super]) are the firmware's RCC configuration. The device stays off the bus until
/// [Driver::connect].
pub fn new<D: Descriptors, T: Instance, const N: usize>(
    _usb: Peri<'static, T>,
    _irq: impl Binding<T::Interrupt, InterruptHandler<T>> + 'static,
    dp: Peri<'static, impl DpPin<T>>,
    dm: Peri<'static, impl DmPin<T>>,
    state: &'static State<N>,
    family: Family,
    hclk_hz: u32,
) -> Driver<D, Mmio> {
    // Safety: embassy_stm32::init turned the PWR clock on, and embassy writes PWR_CR2 in critical sections
    unsafe { family.validate_vddusb() };
    embassy_stm32::rcc::enable_and_reset::<T>();
    let (dp_af, dm_af) = (dp.af_num(), dm.af_num());
    usb_pin(dp, dp_af);
    usb_pin(dm, dm_af);
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
