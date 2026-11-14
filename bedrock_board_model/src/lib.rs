//! The board description: one typed `board.toml` per board that says which MCU pin does what (GPIOs with their
//! level from reset, peripherals with their signals, LEDs, buttons, reserved and shorted pins), for every revision
//! and rework of the board.
//!
//! - [`Description::load`] reads the file with source spans; structural mistakes come back as [`Diagnostic`]s
//!   pointing at the line.
//! - Revisions (`[rev.A]`) and reworks (`[rework.5]`) are patches over the base: an entry they name replaces the
//!   base entry whole, `remove` drops one. [`Description::configurations`] resolves every revision, every
//!   revision with each rework, and with all its reworks, into a complete [`Board`].
//! - [`check()`] runs the checks on each configuration: a pin used twice, a pin the chip's package does not have,
//!   a signal on a pin that cannot carry it ([`ChipData`], from stm32-data), and the description against the PCB's
//!   nets ([`McuPins`], filled by the caller from the board file).
//!
//! The firmware build depends on none of this; `bedrock-board` (crate `bedrock_board_cli`) is the command line.

mod check;
mod chip;
mod load;
mod pcb;
mod schema;
mod source;

pub use check::{Report, check};
pub use chip::{ChipData, ChipError, Package, SignalPin};
pub use load::{DESCRIPTION_BYTES_MAX, Description, LoadError};
pub use pcb::McuPins;
pub use schema::{
    Board, BoardInfo, Button, Entries, Gpio, GpioMode, Led, LedPins, Level, Periph, PinRef, PinUse,
    Pull, Reserved, Short, Use,
};
pub use source::{Diagnostic, Located, Severity, Source, Span};
