//! The driver for the Synopsys OTG core in device mode: STM32H72x / H73x USB1 OTG_HS first, over ULPI with an
//! external high-speed PHY (B135's USB3315) or on the embedded full-speed PHY.
//!
//! - [regs]: the registers, their bits and the [Regs] trait over them ([Mmio] on the chip, `sim` in tests).
//! - [layout]: the FIFO RAM and OUT buffers from the device at compile time, checked against the [Core].
//! - [Driver]: start-up, the bus reset and enumeration in the interrupt handler, the receive FIFO emptied there,
//!   enable and halt with DATA0.
//! - [Ep0] for the control state machine, and the typed endpoints [BulkOut], [BulkIn], [InterruptIn].
//!
//! Slave mode, no DMA. Every rule the code relies on is quoted where it is used, from RM0468 Rev 3 §62 (the
//! OTG_HS chapter); ES0491 Rev 9 (STM32H72x/73x errata) has one OTG_HS erratum, 2.23.1 "Host packet transmission
//! may hang when connecting the full speed interface through a hub to a low-speed device", host mode only.
//!
//! A device on this driver is declared with `descriptors!(Desc = DEVICE, otg = Core::H7_OTG_HS)` and its state
//! with [crate::otg_state!]. Interrupt wiring: with the `embassy-stm32` feature, bind
//! `otg::embassy::InterruptHandler` and create the driver with `otg::embassy::new`; without embassy,
//! [Driver::register] the driver and call [on_interrupt] from the vector.

mod driver;
mod endpoint;
mod ep0;
pub mod layout;
pub mod regs;
pub mod state;

use core::cell::Cell;

use critical_section::Mutex;

pub use driver::{
    DIEPMSK, DOEPMSK, Driver, EndpointError, FORCE_MODE_US, INTERRUPTS, SPIN_LIMIT, TRDT, interrupt,
};
pub use endpoint::{BulkIn, BulkOut, InterruptIn};
pub use ep0::Ep0;
pub use layout::{Core, Layout};
pub use regs::{Mmio, Regs};
pub use state::State;
use state::StateRef;

/// A device's descriptors with its FIFO layout on an OTG core, implemented by
/// `descriptors!(Desc = DEVICE, otg = CORE)`.
pub trait OtgDescriptors: crate::Descriptors {
    const OTG: Layout;
}

/// The PHY the core talks to.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum Phy {
    /// An external PHY on ULPI. `high_speed`: enumerate at high speed when the host can (DSPD 00), else full speed
    /// on it (01). `xcvrdly`: DCFG.XCVRDLY, "Enable delay to default timing, necessary for some ULPI PHYs"
    /// (embassy-usb-synopsys-otg names the Microchip USB334x, which miss the high-speed chirp without it).
    Ulpi { high_speed: bool, xcvrdly: bool },
    /// The embedded full-speed PHY on the OTG_HS_DP / OTG_HS_DM pins (DSPD 11).
    InternalFullSpeed,
}

/// The driver [on_interrupt] serves.
#[derive(Copy, Clone)]
pub(crate) struct Active {
    pub(crate) regs: Mmio,
    pub(crate) layout: &'static Layout,
    pub(crate) state: StateRef,
}

static ACTIVE: Mutex<Cell<Option<Active>>> = Mutex::new(Cell::new(None));

pub(crate) fn register(active: Active) {
    critical_section::with(|cs| ACTIVE.borrow(cs).set(Some(active)));
}

/// The interrupt handler of the [Driver::register]ed driver, for the OTG vector. Does nothing before one is
/// registered.
pub fn on_interrupt() {
    if let Some(a) = critical_section::with(|cs| ACTIVE.borrow(cs).get()) {
        interrupt(&a.regs, a.layout, a.state);
    }
}
