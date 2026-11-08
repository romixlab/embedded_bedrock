//! embassy-stm32 glue for USB1 OTG_HS of the STM32H72x / H73x (feature `embassy-stm32-otg`, with the firmware's
//! embassy-stm32 naming the chip): the driver on embassy's `Peri<USB_OTG_HS>`, its interrupt binding, the RCC
//! enable, the USB supply and the pins. Nothing else of embassy-stm32's USB driver is used.
//!
//! ```ignore
//! bind_interrupts!(struct Irqs {
//!     OTG_HS => bedrock_usb::otg::embassy::InterruptHandler;
//! });
//! bedrock_usb::descriptors!(Desc = DEVICE, otg = bedrock_usb::otg::Core::H7_OTG_HS);
//! bedrock_usb::otg_state!(static USB_STATE: Desc);
//! let ulpi = Ulpi { clk: p.PA5, dir: p.PC2, nxt: p.PC3, stp: p.PC0, d0: p.PA3, /* ... */ };
//! let usb = bedrock_usb::otg::embassy::new_ulpi::<Desc, _, _, _, _, _, _, _, _, _, _, _, _, _, _>(p.USB_OTG_HS, Irqs, ulpi, true, Supply::External, &USB_STATE, 275_000_000);
//! usb.connect();
//! ```
//!
//! The RCC's USB kernel clock (USBSEL: 48 MHz for the embedded full-speed PHY) is the firmware's clock
//! configuration; on ULPI the PHY clocks the core with its own 60 MHz.

use core::marker::PhantomData;

use embassy_stm32::Peri;
use embassy_stm32::gpio::{AfType, Flex, OutputType, Speed};
use embassy_stm32::interrupt::typelevel::{Binding, Handler, Interrupt};
use embassy_stm32::peripherals::USB_OTG_HS;
use embassy_stm32::usb::{
    DmPin, DpPin, Instance, UlpiClkPin, UlpiD0Pin, UlpiD1Pin, UlpiD2Pin, UlpiD3Pin, UlpiD4Pin,
    UlpiD5Pin, UlpiD6Pin, UlpiD7Pin, UlpiDirPin, UlpiNxtPin, UlpiStpPin,
};

use super::{Driver, Mmio, OtgDescriptors, Phy, State};

type Irq = <USB_OTG_HS as Instance>::Interrupt;

/// The OTG_HS interrupt handler for `bind_interrupts!`: calls [super::on_interrupt].
pub struct InterruptHandler {
    _p: PhantomData<()>,
}

impl Handler<Irq> for InterruptHandler {
    unsafe fn on_interrupt() {
        super::on_interrupt()
    }
}

/// Where VDD33USB comes from (RM0468 Rev 3 §6.4.7 "USB regulator").
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum Supply {
    /// "When the USB regulator is disabled through USBREGEN bit, VDD33USB can be provided from an external supply.
    /// In this case VDD33USB and VDD50USB must be connected together."
    External,
    /// "When enabled by USBREGEN bit ..., the VDD33USB is provided from the USB regulator, which is powered through
    /// the VDD50USB pin generally connect to the USB VBUS line."
    Regulator,
}

/// The twelve ULPI pins of an external high-speed PHY.
pub struct Ulpi<
    CLK: UlpiClkPin<USB_OTG_HS>,
    DIR: UlpiDirPin<USB_OTG_HS>,
    NXT: UlpiNxtPin<USB_OTG_HS>,
    STP: UlpiStpPin<USB_OTG_HS>,
    D0: UlpiD0Pin<USB_OTG_HS>,
    D1: UlpiD1Pin<USB_OTG_HS>,
    D2: UlpiD2Pin<USB_OTG_HS>,
    D3: UlpiD3Pin<USB_OTG_HS>,
    D4: UlpiD4Pin<USB_OTG_HS>,
    D5: UlpiD5Pin<USB_OTG_HS>,
    D6: UlpiD6Pin<USB_OTG_HS>,
    D7: UlpiD7Pin<USB_OTG_HS>,
> {
    pub clk: Peri<'static, CLK>,
    pub dir: Peri<'static, DIR>,
    pub nxt: Peri<'static, NXT>,
    pub stp: Peri<'static, STP>,
    pub d0: Peri<'static, D0>,
    pub d1: Peri<'static, D1>,
    pub d2: Peri<'static, D2>,
    pub d3: Peri<'static, D3>,
    pub d4: Peri<'static, D4>,
    pub d5: Peri<'static, D5>,
    pub d6: Peri<'static, D6>,
    pub d7: Peri<'static, D7>,
}

/// A pin in alternate function `af`, push-pull at very high speed (ULPI runs at 60 MHz), for good: the [Flex] is
/// forgotten so its drop doesn't disconnect the pin.
fn af(pin: Peri<'static, impl embassy_stm32::gpio::Pin>, af: u8) {
    let mut f = Flex::new(pin);
    f.set_as_af_unchecked(af, AfType::output(OutputType::PushPull, Speed::VeryHigh));
    core::mem::forget(f);
}

/// The driver on OTG_HS over ULPI: the pins in their alternate functions, then [start] with
/// `Phy::Ulpi { high_speed, xcvrdly: false }`.
#[allow(clippy::too_many_arguments)]
pub fn new_ulpi<
    D: OtgDescriptors,
    const N: usize,
    const W: usize,
    CLK: UlpiClkPin<USB_OTG_HS>,
    DIR: UlpiDirPin<USB_OTG_HS>,
    NXT: UlpiNxtPin<USB_OTG_HS>,
    STP: UlpiStpPin<USB_OTG_HS>,
    D0: UlpiD0Pin<USB_OTG_HS>,
    D1: UlpiD1Pin<USB_OTG_HS>,
    D2: UlpiD2Pin<USB_OTG_HS>,
    D3: UlpiD3Pin<USB_OTG_HS>,
    D4: UlpiD4Pin<USB_OTG_HS>,
    D5: UlpiD5Pin<USB_OTG_HS>,
    D6: UlpiD6Pin<USB_OTG_HS>,
    D7: UlpiD7Pin<USB_OTG_HS>,
>(
    usb: Peri<'static, USB_OTG_HS>,
    irq: impl Binding<Irq, InterruptHandler> + 'static,
    pins: Ulpi<CLK, DIR, NXT, STP, D0, D1, D2, D3, D4, D5, D6, D7>,
    high_speed: bool,
    supply: Supply,
    state: &'static State<N, W>,
    hclk_hz: u32,
) -> Driver<D, Mmio> {
    let a = [
        pins.clk.af_num(),
        pins.dir.af_num(),
        pins.nxt.af_num(),
        pins.stp.af_num(),
        pins.d0.af_num(),
        pins.d1.af_num(),
        pins.d2.af_num(),
        pins.d3.af_num(),
        pins.d4.af_num(),
        pins.d5.af_num(),
        pins.d6.af_num(),
        pins.d7.af_num(),
    ];
    af(pins.clk, a[0]);
    af(pins.dir, a[1]);
    af(pins.nxt, a[2]);
    af(pins.stp, a[3]);
    af(pins.d0, a[4]);
    af(pins.d1, a[5]);
    af(pins.d2, a[6]);
    af(pins.d3, a[7]);
    af(pins.d4, a[8]);
    af(pins.d5, a[9]);
    af(pins.d6, a[10]);
    af(pins.d7, a[11]);
    start(
        usb,
        irq,
        Phy::Ulpi {
            high_speed,
            xcvrdly: false,
        },
        supply,
        state,
        hclk_hz,
    )
}

/// The driver on OTG_HS with its embedded full-speed PHY on `dp` and `dm` (OTG_HS_DP / OTG_HS_DM in their
/// alternate function), then [start] with [Phy::InternalFullSpeed].
pub fn new_fs<D: OtgDescriptors, const N: usize, const W: usize>(
    usb: Peri<'static, USB_OTG_HS>,
    irq: impl Binding<Irq, InterruptHandler> + 'static,
    dp: Peri<'static, impl DpPin<USB_OTG_HS>>,
    dm: Peri<'static, impl DmPin<USB_OTG_HS>>,
    supply: Supply,
    state: &'static State<N, W>,
    hclk_hz: u32,
) -> Driver<D, Mmio> {
    let (p, m) = (dp.af_num(), dm.af_num());
    af(dp, p);
    af(dm, m);
    start(usb, irq, Phy::InternalFullSpeed, supply, state, hclk_hz)
}

/// Enables the USB supply and the core's clocks, registers and starts the driver ([Driver::init]) and unmasks its
/// interrupt; the device stays off the bus until [Driver::connect]. `hclk_hz` is the AHB clock: at least 30 MHz,
/// as GUSBCFG.TRDT 9 asks (RM0468 Rev 3 Table 540), and what [Driver::init]'s 25 ms wait counts in. The binding
/// is the proof that [InterruptHandler] is on the vector, as with embassy's own driver.
///
/// The supply: "Before using VDD33USB, check that it is available by monitoring USB33RDY bit in PWR control
/// register 3 (PWR_CR3). The VDD33USB supply level detector must be enabled through USB33DEN bit" (§6.4.7). The
/// clocks: OTG_HS through embassy's RCC enable and reset, and its ULPI clock (AHB1ENR.USB1OTGHSULPIEN) on ULPI.
pub fn start<D: OtgDescriptors, const N: usize, const W: usize>(
    _usb: Peri<'static, USB_OTG_HS>,
    _irq: impl Binding<Irq, InterruptHandler> + 'static,
    phy: Phy,
    supply: Supply,
    state: &'static State<N, W>,
    hclk_hz: u32,
) -> Driver<D, Mmio> {
    assert!(
        hclk_hz >= 30_000_000,
        "OTG_HS: AHB at 30 MHz or more for TRDT 9"
    );
    let pwr = embassy_stm32::pac::PWR;
    critical_section::with(|_| {
        pwr.cr3().modify(|w| {
            w.set_usb33den(true);
            w.set_usbregen(supply == Supply::Regulator);
        })
    });
    while !pwr.cr3().read().usb33rdy() {}
    let ulpi = matches!(phy, Phy::Ulpi { .. });
    critical_section::with(|_| {
        let rcc = embassy_stm32::pac::RCC;
        rcc.ahb1enr().modify(|w| w.set_usb_otg_hs_ulpien(ulpi));
        rcc.ahb1lpenr().modify(|w| w.set_usb_otg_hs_ulpilpen(ulpi));
    });
    embassy_stm32::rcc::enable_and_reset::<USB_OTG_HS>();
    // Safety: the `Peri` is ours, so are the core's registers and FIFOs
    let regs = unsafe { Mmio::new(embassy_stm32::pac::USB_OTG_HS.as_ptr() as usize, hclk_hz) };
    let usb = Driver::new(regs, state);
    usb.register();
    usb.init(phy);
    Irq::unpend();
    // Safety: the handler is bound (the Binding) and the driver it serves is registered
    unsafe { Irq::enable() };
    usb
}
