//! The driver for the STM32 USB device peripheral "usb_v4" with 32-bit USB SRAM (`usbram_32_2048`: STM32G0B1 /
//! G0C1 first; H5, U0, U3 and C0 parts have the same peripheral).
//!
//! - [regs]: the registers, their bits and the [Regs] trait over them ([Mmio] on the chip, `sim` in tests).
//! - [chep]: values for `USB_CHEPnR` from the value read: invariant writes (R1), toggles as XOR (R2).
//! - [Driver]: start-up (R9), the interrupt handler with the bus reset in it (R8), enable and halt (R12).
//! - [Ep0] for the control state machine, and the typed endpoints [BulkOut], [BulkIn], [InterruptIn] for
//!   `Single` (R3) and `Double` (R4) buffering, with the ES0548 2.16.1 wait before an OUT packet is read (R6).
//!
//! The rules R1-R14 are those of the bedrock_usb design, each quoted where the code relies on it from RM0444 Rev
//! 6, ES0548 Rev 4, DS13560 Rev 6 or USB 2.0.
//!
//! Interrupt wiring: with the `embassy-stm32` feature, bind `embassy::InterruptHandler` in `bind_interrupts!`
//! (on the G0 it shares `USB_UCPD1_2` with UCPD) and create the driver with `embassy::new`. Without embassy,
//! [Driver::register] the driver and call [on_interrupt] from the vector.

pub mod chep;
mod driver;
pub(crate) mod endpoint;
mod ep0;
pub mod regs;
pub mod state;

#[cfg(any(test, feature = "sim"))]
pub mod sim;

#[cfg(feature = "embassy-stm32")]
pub mod embassy;

#[cfg(test)]
mod tests;

use core::cell::UnsafeCell;
use core::sync::atomic::{AtomicBool, Ordering};

pub use driver::{
    Driver, EndpointError, INTERRUPTS, OUT_PACKET_DELAY_NS, SOF_INTERRUPTS, T_STARTUP_NS, interrupt,
};
pub use endpoint::{BulkIn, BulkOut, InterruptIn};
pub use ep0::{Ep0, Ep0Error};
pub use regs::{Mmio, Regs};
pub use state::{Sof, State, Status, event};

use crate::layout::Layout;
use state::StateRef;

/// The driver an `on_interrupt` serves ([on_interrupt] here, `v3::on_interrupt` for usb_v3).
#[derive(Copy, Clone)]
pub(crate) struct Active<R> {
    pub(crate) regs: R,
    pub(crate) layout: &'static Layout,
    pub(crate) state: StateRef,
}

/// A registration, written once before the interrupt is unmasked and only read after. Not a
/// `Mutex<Cell<Option<Active>>>`: reading that copies the 28 bytes twice through `memcpy` on every interrupt
/// (226 cycles at opt-level z on the Cortex-M0+), before the handler has looked at a flag or timestamped a SOF.
pub(crate) struct Registration<R> {
    active: UnsafeCell<Option<Active<R>>>,
    set: AtomicBool,
}

// Safety: `active` is written once, in a critical section, before `set` is stored with Release; readers load
// `set` with Acquire first and never write.
unsafe impl<R: Sync> Sync for Registration<R> {}

impl<R> Registration<R> {
    pub(crate) const fn new() -> Self {
        Registration {
            active: UnsafeCell::new(None),
            set: AtomicBool::new(false),
        }
    }

    /// Panics on a second registration: one peripheral, one driver.
    pub(crate) fn set(&self, active: Active<R>) {
        critical_section::with(|_| {
            assert!(
                !self.set.load(Ordering::Relaxed),
                "bedrock_usb: a driver for this peripheral is registered already"
            );
            // Safety: not set yet, so no reader looks at it
            unsafe { *self.active.get() = Some(active) };
            self.set.store(true, Ordering::Release);
        })
    }

    /// The registered driver, `None` before one is.
    #[inline(always)]
    pub(crate) fn get(&self) -> Option<&Active<R>> {
        if self.set.load(Ordering::Acquire) {
            // Safety: set, so written for good
            unsafe { (*self.active.get()).as_ref() }
        } else {
            None
        }
    }
}

static ACTIVE: Registration<Mmio> = Registration::new();

pub(crate) fn register(active: Active<Mmio>) {
    ACTIVE.set(active)
}

/// The interrupt handler of the [Driver::register]ed driver, for the USB vector. Does nothing before one is
/// registered, and returns at once when no USB flag is pending.
pub fn on_interrupt() {
    if let Some(a) = ACTIVE.get() {
        interrupt(&a.regs, a.layout, a.state);
    }
}
