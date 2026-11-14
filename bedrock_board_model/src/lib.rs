//! The board description: one typed `board.toml` per board that says which MCU pin does what (GPIOs with their
//! level from reset, peripherals with their signals, LEDs, buttons, reserved and shorted pins), for every revision
//! and rework of the board.
//!
//! - [`Description::load`] reads the file with source spans; structural mistakes come back as [`Diagnostic`]s
//!   pointing at the line.
//! - Revisions (`[rev.A]`) and reworks (`[rework.5]`) are patches over the base: an entry they name replaces the
//!   base entry whole, `remove` drops one. [`Description::configurations`] resolves every revision, every
//!   revision with each rework, and with all its reworks, into a complete [`Board`].
//!
//! This crate is the schema's one loader. The checks of a description against itself, the chip's data and the
//! PCB's nets are spirit_level's board lints (`spl check`, the embedded tier's `brd-*`), which read the file
//! through this crate.

mod load;
mod schema;
mod source;

pub use load::{DESCRIPTION_BYTES_MAX, Description, LoadError};
pub use schema::{
    Board, BoardInfo, Button, Entries, Gpio, GpioMode, Led, LedPins, Level, Periph, PinRef, PinUse,
    Pull, Reserved, Short, Use,
};
pub use source::{Diagnostic, Located, Source, Span};
