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
mod endpoint;
mod ep0;
pub mod regs;
pub mod state;

#[cfg(any(test, feature = "sim"))]
pub mod sim;

#[cfg(feature = "embassy-stm32")]
pub mod embassy;

#[cfg(test)]
mod tests;

use core::cell::Cell;

use critical_section::Mutex;

pub use driver::{Driver, EndpointError, INTERRUPTS, OUT_PACKET_DELAY_NS, T_STARTUP_NS, interrupt};
pub use endpoint::{BulkIn, BulkOut, InterruptIn};
pub use ep0::{Ep0, Ep0Error};
pub use regs::{Mmio, Regs};
pub use state::{State, event};

use crate::layout::Layout;
use state::StateRef;

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

/// The interrupt handler of the [Driver::register]ed driver, for the USB vector. Does nothing before one is
/// registered, and returns at once when no USB flag is pending.
pub fn on_interrupt() {
    if let Some(a) = critical_section::with(|cs| ACTIVE.borrow(cs).get()) {
        interrupt(&a.regs, a.layout, a.state);
    }
}
