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
//! with [crate::otg_state!]. Interrupt wiring: with the `embassy-stm32-otg` feature, bind
//! `otg::embassy::InterruptHandler` and create the driver with `otg::embassy::new_ulpi` or `new_fs`; without embassy,
//! [Driver::register] the driver and call [on_interrupt] from the vector.

mod driver;
mod endpoint;
mod ep0;
pub mod layout;
pub mod regs;
pub mod state;

#[cfg(any(test, feature = "sim"))]
pub mod sim;

#[cfg(feature = "embassy-stm32-otg")]
pub mod embassy;

#[cfg(test)]
mod tests;

use core::cell::UnsafeCell;
use core::sync::atomic::{AtomicBool, Ordering};

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

/// The registration, written once before the interrupt is unmasked and only read after: what `v4` does, so the
/// handler doesn't copy it out of a `Mutex<Cell<_>>` on every interrupt.
struct Registration {
    active: UnsafeCell<Option<Active>>,
    set: AtomicBool,
}

// Safety: `active` is written once, in a critical section, before `set` is stored with Release; readers load
// `set` with Acquire first and never write.
unsafe impl Sync for Registration {}

static ACTIVE: Registration = Registration {
    active: UnsafeCell::new(None),
    set: AtomicBool::new(false),
};

/// Panics on a second registration: one OTG core, one driver.
pub(crate) fn register(active: Active) {
    critical_section::with(|_| {
        assert!(
            !ACTIVE.set.load(Ordering::Relaxed),
            "bedrock_usb: an OTG driver is registered already"
        );
        // Safety: not set yet, so no reader looks at it (see Registration)
        unsafe { *ACTIVE.active.get() = Some(active) };
        ACTIVE.set.store(true, Ordering::Release);
    })
}

/// The interrupt handler of the [Driver::register]ed driver, for the OTG vector. Does nothing before one is
/// registered.
pub fn on_interrupt() {
    if ACTIVE.set.load(Ordering::Acquire) {
        // Safety: set, so written for good (see Registration)
        if let Some(a) = unsafe { &*ACTIVE.active.get() } {
            interrupt(&a.regs, a.layout, a.state);
        }
    }
}
