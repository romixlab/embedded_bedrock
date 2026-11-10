#![cfg_attr(not(test), no_std)]
//! Applies a clock plan at boot (embedded_bedrock RCC-4).
//!
//! The plan is worked out on the host by `bedrock_clocks` and committed as `clocks.rs` next to the board's
//! `clocks.toml`: register words and exact frequencies, nothing to compute here. Each family's `apply`
//! ([h72x::apply], [g0::apply]) writes the words in the reference manual's order and waits, with a bound, for each
//! ready flag; its `verify` reads every register back and names the first one that differs. No PAC: the few
//! registers are addressed directly (the addresses are held to stm32-metapac in host tests), and all access goes
//! through [Regs] so the sequence runs against a simulated chip in the host tests.

pub mod g0;
pub mod h72x;

/// Register access. [Mmio] on the chip; a simulated chip in the host tests.
pub trait Regs {
    fn read(&mut self, addr: u32) -> u32;
    fn write(&mut self, addr: u32, value: u32);
}

/// The chip's own registers.
pub struct Mmio;

impl Regs for Mmio {
    #[inline(always)]
    fn read(&mut self, addr: u32) -> u32 {
        // SAFETY: only the RCC / PWR / FLASH / CRS register addresses of a family's `addr` reach here
        unsafe { core::ptr::read_volatile(addr as *const u32) }
    }
    #[inline(always)]
    fn write(&mut self, addr: u32, value: u32) {
        // SAFETY: as above
        unsafe { core::ptr::write_volatile(addr as *mut u32, value) }
    }
}

/// The generated `clocks.rs` names its exact frequencies through this, so firmware needs no exact_units dependency
/// of its own.
pub use exact_units;
