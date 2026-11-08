//! embassy-stm32 glue for usb_v1 / usb_v2 (F1, F3, L1): the driver on embassy's `Peri<USB>`, its interrupt
//! binding, RCC enable, the pins and a GPIO-switched DP pull-up. Nothing else of embassy-stm32's USB driver is
//! used.
//!
//! ```ignore
//! bind_interrupts!(struct Irqs {
//!     USB_LP_CAN1_RX0 => bedrock_usb::v1::embassy::InterruptHandler<peripherals::USB>; // the F1's vector
//! });
//! bedrock_usb::state!(static USB_STATE: Desc);
//! let usb = bedrock_usb::v1::embassy::new::<Desc, _, Spaced512, _>(
//!     p.USB, Irqs, p.PA12, p.PA11, &USB_STATE, PullUp::External, 72_000_000,
//! );
//! usb.connect();
//! ```
//!
//! On the F1, enable bedrock_usb's `stm32f1` feature: embassy-stm32 maps pins through AFIO there, and its USB pin
//! traits have no alternate-function number. DP and DM need none on the F1: "As soon as the USB is enabled, these
//! pins are automatically connected to the USB internal transceiver" (RM0008 Rev 21 Table 29 "USB").

use core::marker::PhantomData;

use embassy_stm32::Peri;
use embassy_stm32::gpio::{AnyPin, Level, Output, Pin, Speed};
use embassy_stm32::interrupt::typelevel::{Binding, Handler, Interrupt};
use embassy_stm32::usb::{DmPin, DpPin, Instance};

use super::{Access, Driver, Mmio, PullUp, Spaced512, State};
use crate::Descriptors;

/// The USB interrupt handler for `bind_interrupts!`, on the low-priority vector: calls [super::on_interrupt] for
/// `A`, the chip's [Access].
pub struct InterruptHandler<T: Instance, A: Access = Spaced512> {
    _p: PhantomData<(T, A)>,
}

impl<T: Instance, A: Access> Handler<T::Interrupt> for InterruptHandler<T, A> {
    unsafe fn on_interrupt() {
        super::on_interrupt::<A>()
    }
}

/// A DP or DM pin in the mode its chip wants: on its alternate function where it lists USB as one (the F3:
/// AF14; embassy's `af_num()` is 0 for a pin without one, and no chip here has USB on AF0), left alone on the F1
/// (feature `stm32f1`). The pin stays ours: the `Flex` is forgotten, so its drop doesn't disconnect it.
#[cfg(not(feature = "stm32f1"))]
fn usb_pin(pin: Peri<'static, impl Pin>, af: u8) {
    use embassy_stm32::gpio::{AfType, Flex, OutputType};
    if af != 0 {
        let mut flex = Flex::new(pin);
        flex.set_as_af_unchecked(af, AfType::output(OutputType::PushPull, Speed::VeryHigh));
        core::mem::forget(flex);
    }
}

/// The DP pull-up switched by `pin`, a push-pull output set up here with the resistor off. `active_high`: the
/// level that connects it (a PNP transistor or a P-MOSFET to 3.3 V is active low). The output is never dropped.
pub fn gpio_pull_up(pin: Peri<'static, impl Pin>, active_high: bool) -> PullUp {
    let pin: Peri<'static, AnyPin> = pin.into();
    let n = pin.pin();
    let bsrr = pin.block().bsrr().as_ptr() as usize;
    let off = if active_high { Level::Low } else { Level::High };
    core::mem::forget(Output::new(pin, off, Speed::Low));
    PullUp::Gpio {
        bsrr,
        pin: n,
        active_high,
    }
}

/// Takes the peripheral, enables the USB clock, sets the pins, registers and starts the driver ([Driver::init])
/// and unmasks its interrupt. The binding is the proof that [InterruptHandler] is on the vector, as with embassy's
/// own driver. `A` is the chip's USB SRAM [Access] (checked against the PAC's spacing); `pull_up` what connects
/// DP; `hclk_hz` the core clock the tSTARTUP and OUT-packet waits count in. The 48 MHz USB clock (on the F1 the
/// PLL at 72 MHz with the USB prescaler at 1.5) and an APB1 clock of at least 13 MHz on the F1, 10 MHz on the L1
/// (the errata in [super]) are the firmware's RCC configuration. The device stays off the bus until
/// [Driver::connect] (with [PullUp::External], the resistor already has it on).
pub fn new<D: Descriptors, T: Instance, A: Access, const N: usize>(
    _usb: Peri<'static, T>,
    _irq: impl Binding<T::Interrupt, InterruptHandler<T, A>> + 'static,
    dp: Peri<'static, impl DpPin<T>>,
    dm: Peri<'static, impl DmPin<T>>,
    state: &'static State<N>,
    pull_up: PullUp,
    hclk_hz: u32,
) -> Driver<D, Mmio<A>> {
    let sram = embassy_stm32::pac::USBRAM;
    assert_eq!(
        sram.mem(1).as_ptr() as usize - sram.as_ptr() as usize,
        A::STRIDE,
        "the Access type doesn't match this chip's USB SRAM"
    );
    embassy_stm32::rcc::enable_and_reset::<T>();
    #[cfg(not(feature = "stm32f1"))]
    {
        let (dp_af, dm_af) = (dp.af_num(), dm.af_num());
        usb_pin(dp, dp_af);
        usb_pin(dm, dm_af);
    }
    #[cfg(feature = "stm32f1")]
    let _ = (dp, dm);
    // Safety: the `Peri` is ours, so are its registers and USB SRAM; the pull-up is the caller's to give
    let regs = unsafe {
        Mmio::<A>::at(
            embassy_stm32::pac::USB.as_ptr() as usize,
            sram.as_ptr() as usize,
            hclk_hz,
            pull_up,
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
