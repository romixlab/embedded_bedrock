#![cfg_attr(not(test), no_std)]
//! Applies a clock plan at boot (embedded_bedrock RCC-4).
//!
//! The plan is worked out on the host by `bedrock_clocks` and committed as `clocks.rs` next to the board's
//! `clocks.toml`: register words and exact frequencies, nothing to compute here. [h72x::apply] writes the words in
//! the reference manual's order and waits, with a bound, for each ready flag; [h72x::verify] reads every register
//! back and names the first one that differs. No PAC: the few registers are addressed directly (the addresses are
//! held to stm32-metapac in a host test), and all access goes through [h72x::Regs] so the sequence runs against a
//! simulated chip in the host tests.

pub mod h72x;

/// The generated `clocks.rs` names its exact frequencies through this, so firmware needs no exact_units dependency
/// of its own.
pub use exact_units;
