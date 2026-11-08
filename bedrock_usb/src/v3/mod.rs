//! The usb_v4 driver ([crate::v4]) on the STM32 USB device peripheral "usb_v3" with 16-bit USB SRAM
//! (`usbram_16x2_1024`): STM32L4 (L412 / L422, L43x / L44x, L45x / L46x), STM32G4 and STM32WB.
//!
//! The two peripherals differ in three places, and only there; the driver, its endpoints, [crate::Usb] and the
//! layout are the same code:
//!
//! - **USB SRAM access.** RM0440 Rev 9 §45.6.2: "The packet memory should be accessed only by byte (8-bit) or
//!   half-word (16-bit) accesses. Word (32-bit) accesses are not allowed." Its 1024 bytes are "structured as 512
//!   half-words of 16 bits" (§45.4.1), at consecutive addresses ("Dedicated packet buffer memory SRAM access
//!   scheme: 2 x 16 bits / word", Table 422). [Mmio] makes each 32-bit word of the driver two half-word accesses,
//!   low half first.
//! - **Buffer descriptor table.** Four half-words per register at `[USB_BTABLE] + n*8`: ADDRn_TX, COUNTn_TX
//!   (`+ 2`), ADDRn_RX (`+ 4`), COUNTn_RX (`+ 6`, BL_SIZE bit 15, NUM_BLOCK bits 14:10, COUNT bits 9:0). As
//!   bytes that is exactly usb_v4's two words per register (address in the low half, count and the receive size
//!   in the high half), so [crate::layout::Layout]'s `bd` values hold for both. `USB_BTABLE` (offset 0x50) is set
//!   to 0, where the layout puts the table.
//! - **Size.** 1024 bytes instead of 2048: `Driver::new` fails the build when a device's buffers need more
//!   ([Regs::SRAM_SIZE]).
//!
//! ```compile_fail
//! # use bedrock_usb::desc::{Device, Endpoint, Function, Interface, Item};
//! # use bedrock_usb::Buffering::Double;
//! // seven double-buffered 64-byte endpoints: 1088 bytes of buffer table and buffers, fine on usb_v4
//! const EPS: &[Item] = &[
//!     Item::Endpoint(Endpoint::bulk_out(1, 64, Double)), Item::Endpoint(Endpoint::bulk_in(2, 64, Double)),
//!     Item::Endpoint(Endpoint::bulk_out(3, 64, Double)), Item::Endpoint(Endpoint::bulk_in(4, 64, Double)),
//!     Item::Endpoint(Endpoint::bulk_out(5, 64, Double)), Item::Endpoint(Endpoint::bulk_in(6, 64, Double)),
//!     Item::Endpoint(Endpoint::bulk_out(7, 64, Double)),
//! ];
//! const DEVICE: Device = Device {
//!     functions: &[Function { interfaces: &[Interface { items: EPS, ..Interface::vendor() }], ..Function::vendor() }],
//!     ..Device::new(0xc0de, 0xcafe)
//! };
//! bedrock_usb::descriptors!(Desc = DEVICE);
//! bedrock_usb::state!(static STATE: Desc);
//! fn start(regs: bedrock_usb::v3::Mmio) -> bedrock_usb::v3::Driver<Desc, bedrock_usb::v3::Mmio> {
//!     bedrock_usb::v3::Driver::new(regs, &STATE) // the buffers don't fit in usb_v3's 1024 bytes
//! }
//! if false {
//!     start(unsafe { bedrock_usb::v3::Mmio::of(bedrock_usb::v3::Family::G4, 170_000_000) });
//! }
//! ```
//!
//! The same device on usb_v4 builds:
//!
//! ```
//! # use bedrock_usb::desc::{Device, Endpoint, Function, Interface, Item};
//! # use bedrock_usb::Buffering::Double;
//! # const EPS: &[Item] = &[
//! #     Item::Endpoint(Endpoint::bulk_out(1, 64, Double)), Item::Endpoint(Endpoint::bulk_in(2, 64, Double)),
//! #     Item::Endpoint(Endpoint::bulk_out(3, 64, Double)), Item::Endpoint(Endpoint::bulk_in(4, 64, Double)),
//! #     Item::Endpoint(Endpoint::bulk_out(5, 64, Double)), Item::Endpoint(Endpoint::bulk_in(6, 64, Double)),
//! #     Item::Endpoint(Endpoint::bulk_out(7, 64, Double)),
//! # ];
//! # const DEVICE: Device = Device {
//! #     functions: &[Function { interfaces: &[Interface { items: EPS, ..Interface::vendor() }], ..Function::vendor() }],
//! #     ..Device::new(0xc0de, 0xcafe)
//! # };
//! bedrock_usb::descriptors!(Desc = DEVICE);
//! bedrock_usb::state!(static STATE: Desc);
//! fn start(regs: bedrock_usb::v4::Mmio) -> bedrock_usb::v4::Driver<Desc, bedrock_usb::v4::Mmio> {
//!     bedrock_usb::v4::Driver::new(regs, &STATE)
//! }
//! if false {
//!     start(unsafe { bedrock_usb::v4::Mmio::g0(64_000_000) });
//! }
//! ```
//!
//! The registers are usb_v4's low 16 bits under other names (RM0440 §45.6: CTR_RX / CTR_TX for VTRX / VTTX, FRES
//! for USBRST, LP_MODE / FSUSP for SUSPRDY / SUSPEN, RESETM / RESET for RST_DCONM / RST_DCON), the upper 16
//! "Reserved". [Mmio] writes only the low 16 bits, so usb_v4's host-mode `rc_w0` bits never reach them.
//! `tests/v3_metapac.rs` holds offsets and bits to stm32-metapac's `usb_v3` and `usbram_16x2_1024` (STM32G474).
//!
//! Errata (each quoted from the sheet):
//! - ES0387 Rev 11 (STM32L451 / L452 / L462, rev Y) §2.22.4 "Buffer description table update completes after
//!   CTR interrupt triggers", the G0B1's ES0548 §2.16.1 word for word, workaround "a small delay ... 800 ns in
//!   Full Speed mode": the driver's [OUT_PACKET_DELAY_NS] wait, kept on every usb_v3 part. ES0430 Rev 10 (G431 /
//!   G441), ES0431 Rev 10 (G47x / G48x), ES0456 Rev 6 (L412 / L422) and ES0394 Rev 18 (WB55 / WB35) don't list it.
//! - "Possible packet memory overrun/underrun at low APB frequency" (ES0456 §2.17.1, ES0387 §2.22.1, ES0394
//!   §2.17.1): "10 MHz minimum APB clock frequency is required". The firmware's clock tree.
//! - "ESOF interrupt timing desynchronized after resume signaling" (ES0430 §2.20.1, ES0431 §2.16.1, ES0456
//!   §2.17.2, ES0387 §2.22.2, ES0394 §2.17.2): only after a device-initiated resume; the driver has no remote
//!   wake-up, so nothing to do until it does (then: SUSPM masked for 3 ms).
//! - "Incorrect CRC16 in the memory buffer" (ES0430 §2.20.2 and the others): the CRC goes into the receive
//!   buffer after a short packet; the driver reads COUNTn_RX bytes only, and an OUT buffer is exactly the max
//!   packet size, so the CRC never lands outside it.
//! - "Device not responding to USB messages to its endpoint 0xF" (ES0394 §2.17.4, WB): endpoint numbers here are
//!   0 to 7.
//!
//! The L4 and WB also need VDDUSB validated before the peripheral works: "Setting this bit is mandatory to use the
//! USB FS peripheral" (PWR_CR2 USV, RM0394 Rev 5 §5.4.2, RM0434 Rev 16 §5.6.2): [Family::validate_vddusb]. The
//! G4 has no such switch.
//!
//! Interrupt wiring: with the `embassy-stm32` feature, bind `v3::embassy::InterruptHandler` (`USB_LP` on the G4
//! and WB, `USB` on the L4) and create the driver with `v3::embassy::new`. Without embassy, [Driver::register]
//! the driver and call [on_interrupt] from that vector. The G4 and WB also have `USB_HP`, which stays disabled:
//! with CTRM set every transfer raises the generic request on `USB_LP` too ("The CTR bit is set by the hardware as
//! soon as an endpoint successfully completes a transaction, generating a generic interrupt request if the
//! corresponding bit in USB_CNTR is set. An endpoint dedicated interrupt condition is activated independently
//! from the CTRM bit", RM0440 §45.6.1 USB_ISTR).

#[cfg(feature = "embassy-stm32")]
pub mod embassy;

#[cfg(test)]
mod tests;

use crate::Descriptors;
use crate::v4::Registration;
pub use crate::v4::{
    BulkIn, BulkOut, Driver, EndpointError, Ep0, Ep0Error, INTERRUPTS, InterruptIn,
    OUT_PACKET_DELAY_NS, Regs, SOF_INTERRUPTS, Sof, State, Status, T_STARTUP_NS, event, interrupt,
};

/// Register offsets usb_v3 has beyond usb_v4's ([crate::v4::regs::offset]).
pub mod offset {
    /// `USB_BTABLE`, "Buffer table address" (RM0440 §45.6.1).
    pub const BTABLE: usize = 0x50;
}

/// Bytes of USB SRAM: "Size of dedicated packet buffer memory SRAM: 1024 bytes" (RM0440 Rev 9 Table 422, RM0394
/// Rev 5 Table 282, RM0434 Rev 16 Table 253).
pub const SRAM_SIZE: usize = 1024;

/// The bits of a usb_v3 register: the low half-word.
const REG_MASK: u32 = 0xffff;

/// The usb_v3 peripheral's registers and USB SRAM in memory, with the core clock for [Regs::delay_ns].
#[derive(Copy, Clone, Debug)]
pub struct Mmio {
    usb: usize,
    sram: usize,
    /// The core clock in MHz, rounded up.
    mhz: u32,
}

impl Mmio {
    /// USB FS at 0x4000_5C00, USB SRAM at 0x4000_6000 (RM0440 Rev 9 Table 3 "Memory map and peripheral
    /// register boundary addresses").
    pub const G4_USB: usize = 0x4000_5c00;
    pub const G4_SRAM: usize = 0x4000_6000;
    /// USB FS at 0x4000_6800, USB SRAM at 0x4000_6C00 (RM0394 Rev 5 Table 2 "STM32L41xxx/42xxx/43xxx/44xxx/45xxx/46xxx
    /// memory map and peripheral register boundary addresses").
    pub const L4_USB: usize = 0x4000_6800;
    pub const L4_SRAM: usize = 0x4000_6c00;
    /// USB FS at 0x4000_6800, USB SRAM at 0x4000_6C00 (RM0434 Rev 16 Table 1 "Memory map and peripheral register
    /// boundary addresses").
    pub const WB_USB: usize = 0x4000_6800;
    pub const WB_SRAM: usize = 0x4000_6c00;

    /// The peripheral of `family`, `hclk_hz` the core clock the delays count in.
    ///
    /// # Safety
    /// The caller owns the USB peripheral: nothing else writes its registers or its SRAM.
    pub const unsafe fn of(family: Family, hclk_hz: u32) -> Self {
        let (usb, sram) = match family {
            Family::G4 => (Self::G4_USB, Self::G4_SRAM),
            Family::L4 => (Self::L4_USB, Self::L4_SRAM),
            Family::Wb => (Self::WB_USB, Self::WB_SRAM),
        };
        unsafe { Self::new(usb, sram, hclk_hz) }
    }

    /// A usb_v3 peripheral at other addresses (the L5 has one at 0x4000_D400 / 0x4000_D800).
    ///
    /// # Safety
    /// `usb` and `sram` are the peripheral's register block and its USB SRAM, and the caller owns them.
    pub const unsafe fn new(usb: usize, sram: usize, hclk_hz: u32) -> Self {
        Mmio {
            usb,
            sram,
            mhz: hclk_hz.div_ceil(1_000_000),
        }
    }

    fn reg(&self, offset: usize) -> *mut u32 {
        (self.usb + offset) as *mut u32
    }

    #[inline(always)]
    fn read(&self, offset: usize) -> u32 {
        unsafe { self.reg(offset).read_volatile() }
    }

    #[inline(always)]
    fn write(&self, offset: usize, value: u32) {
        unsafe { self.reg(offset).write_volatile(value & REG_MASK) }
    }

    fn half(&self, index: usize) -> *mut u16 {
        (self.sram + 2 * index) as *mut u16
    }
}

impl Regs for Mmio {
    const SRAM_SIZE: usize = SRAM_SIZE;

    #[inline(always)]
    fn chep(&self, n: usize) -> u32 {
        self.read(crate::v4::regs::offset::CHEP0R + 4 * n)
    }
    #[inline(always)]
    fn set_chep(&self, n: usize, value: u32) {
        self.write(crate::v4::regs::offset::CHEP0R + 4 * n, value)
    }
    #[inline(always)]
    fn cntr(&self) -> u32 {
        self.read(crate::v4::regs::offset::CNTR)
    }
    #[inline(always)]
    fn set_cntr(&self, value: u32) {
        self.write(crate::v4::regs::offset::CNTR, value)
    }
    #[inline(always)]
    fn istr(&self) -> u32 {
        self.read(crate::v4::regs::offset::ISTR)
    }
    #[inline(always)]
    fn set_istr(&self, value: u32) {
        self.write(crate::v4::regs::offset::ISTR, value)
    }
    #[inline(always)]
    fn fnr(&self) -> u32 {
        self.read(crate::v4::regs::offset::FNR)
    }
    #[inline(always)]
    fn daddr(&self) -> u32 {
        self.read(crate::v4::regs::offset::DADDR)
    }
    #[inline(always)]
    fn set_daddr(&self, value: u32) {
        self.write(crate::v4::regs::offset::DADDR, value)
    }
    #[inline(always)]
    fn bcdr(&self) -> u32 {
        self.read(crate::v4::regs::offset::BCDR)
    }
    #[inline(always)]
    fn set_bcdr(&self, value: u32) {
        self.write(crate::v4::regs::offset::BCDR, value)
    }
    /// Half-words `2 * index` (low) and `2 * index + 1` (high).
    #[inline(always)]
    fn sram(&self, index: usize) -> u32 {
        unsafe {
            let lo = self.half(2 * index).read_volatile() as u32;
            let hi = self.half(2 * index + 1).read_volatile() as u32;
            lo | (hi << 16)
        }
    }
    #[inline(always)]
    fn set_sram(&self, index: usize, value: u32) {
        unsafe {
            self.half(2 * index).write_volatile(value as u16);
            self.half(2 * index + 1)
                .write_volatile((value >> 16) as u16);
        }
    }
    #[inline(always)]
    fn set_btable(&self, value: u32) {
        self.write(offset::BTABLE, value)
    }
    fn delay_ns(&self, ns: u32) {
        crate::v4::regs::delay_ns(ns, self.mhz);
    }
}

/// The chip families with usb_v3 and 16-bit USB SRAM this module names addresses for.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum Family {
    G4,
    L4,
    Wb,
}

impl Family {
    /// `PWR_CR2` (offset 0x04) of the L4 (PWR at 0x4000_7000, RM0394 Rev 5 Table 2) and the WB (PWR at
    /// 0x5800_0400, RM0434 Rev 16 Table 1); the G4 has no USV.
    pub const fn pwr_cr2(self) -> Option<usize> {
        match self {
            Family::G4 => None,
            Family::L4 => Some(0x4000_7004),
            Family::Wb => Some(0x5800_0404),
        }
    }

    /// `PWR_CR2` bit 10, USV: "VDDUSB USB supply valid ... Setting this bit is mandatory to use the USB FS
    /// peripheral" (RM0394 §5.4.2, RM0434 §5.6.2).
    pub const USV: u32 = 1 << 10;

    /// Sets USV on the L4 and WB (a read-modify-write in a critical section, the PWR clock on), nothing on the G4.
    ///
    /// # Safety
    /// The PWR peripheral's clock is on, and nothing else writes `PWR_CR2` outside a critical section.
    pub unsafe fn validate_vddusb(self) {
        if let Some(cr2) = self.pwr_cr2() {
            let cr2 = cr2 as *mut u32;
            critical_section::with(|_| unsafe {
                cr2.write_volatile(cr2.read_volatile() | Self::USV)
            });
        }
    }
}

static ACTIVE: Registration<Mmio> = Registration::new();

impl<D: Descriptors> Driver<D, Mmio> {
    /// Makes this driver the one [on_interrupt] serves. Call it before unmasking the interrupt.
    pub fn register(&self) {
        ACTIVE.set(self.active())
    }
}

/// The interrupt handler of the [Driver::register]ed usb_v3 driver, for the USB vector. Does nothing before one
/// is registered, and returns at once when no USB flag is pending.
pub fn on_interrupt() {
    if let Some(a) = ACTIVE.get() {
        interrupt(&a.regs, a.layout, a.state);
    }
}
