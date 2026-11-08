//! The registers the driver touches, their bits, and the [Regs] trait that reaches them: [Mmio] on the chip, the
//! simulated chip (`v4::sim`, feature `sim`) in host tests.
//!
//! Offsets and bits are RM0444 Rev 6 §37.6 and §37.7; `tests/v4_metapac.rs` holds every one of them to
//! stm32-metapac's `usb_v4` and `usbram_32_2048`.

/// Register offsets from the peripheral base.
pub mod offset {
    /// `USB_CHEPnR`: "Address offset: 0x00 + 0x4 * n, (n = 0 to 7)" (§37.6.7).
    pub const CHEP0R: usize = 0x00;
    pub const CNTR: usize = 0x40;
    pub const ISTR: usize = 0x44;
    pub const FNR: usize = 0x48;
    pub const DADDR: usize = 0x4c;
    pub const BCDR: usize = 0x58;
}

/// `USB_CHEPnR` bits (§37.6.7).
pub mod chep {
    pub const EA: u32 = 0x0f;
    pub const STATTX_SHIFT: u32 = 4;
    pub const STATTX: u32 = 0b11 << STATTX_SHIFT;
    pub const DTOGTX: u32 = 1 << 6;
    pub const VTTX: u32 = 1 << 7;
    /// `EPKIND`: `DBL_BUF` on a bulk endpoint, `STATUS_OUT` on a control endpoint (Table 236).
    pub const EPKIND: u32 = 1 << 8;
    pub const UTYPE_SHIFT: u32 = 9;
    pub const UTYPE: u32 = 0b11 << UTYPE_SHIFT;
    pub const SETUP: u32 = 1 << 11;
    pub const STATRX_SHIFT: u32 = 12;
    pub const STATRX: u32 = 0b11 << STATRX_SHIFT;
    pub const DTOGRX: u32 = 1 << 14;
    pub const VTRX: u32 = 1 << 15;
    /// Host-mode `rc_w0` bits: NAK (23), ERR_TX (25), ERR_RX (26), THREE_ERR_TX (28:27), THREE_ERR_RX (30:29).
    pub const HOST_RC_W0: u32 = (1 << 23) | (0b11_1111 << 25);

    /// The bits that toggle when written with 1 and stay when written with 0 (R2).
    pub const TOGGLE: u32 = STATTX | DTOGTX | STATRX | DTOGRX;
    /// The bits only 0 can be written to (R1): a write puts 1 there unless it means to clear.
    pub const RC_W0: u32 = VTRX | VTTX | HOST_RC_W0;
}

/// `USB_CNTR` bits (§37.6.1). The reference manual's USBRST, PDWN, SUSPRDY, SUSPEN and RST_DCONM are named here
/// as the manual's register table names them.
pub mod cntr {
    /// USBRST: the peripheral is held in reset while set.
    pub const USBRST: u32 = 1 << 0;
    /// PDWN: the transceiver is powered down while set.
    pub const PDWN: u32 = 1 << 1;
    /// SUSPRDY: the analog transceiver in low-power mode.
    pub const SUSPRDY: u32 = 1 << 2;
    /// SUSPEN: suspend mode, the SOF check off.
    pub const SUSPEN: u32 = 1 << 3;
    pub const L2RES: u32 = 1 << 4;
    pub const ESOFM: u32 = 1 << 8;
    pub const SOFM: u32 = 1 << 9;
    pub const RST_DCONM: u32 = 1 << 10;
    pub const SUSPM: u32 = 1 << 11;
    pub const WKUPM: u32 = 1 << 12;
    pub const ERRM: u32 = 1 << 13;
    pub const PMAOVRM: u32 = 1 << 14;
    pub const CTRM: u32 = 1 << 15;
}

/// `USB_ISTR` bits (§37.6.2). Every flag is `rc_w0` except CTR, IDN and DIR, which are read only.
pub mod istr {
    pub const IDN: u32 = 0x0f;
    /// DIR: 1 when the pending transfer of IDN's register is an OUT (VTRX set).
    pub const DIR: u32 = 1 << 4;
    pub const ESOF: u32 = 1 << 8;
    pub const SOF: u32 = 1 << 9;
    pub const RST_DCON: u32 = 1 << 10;
    pub const SUSP: u32 = 1 << 11;
    pub const WKUP: u32 = 1 << 12;
    pub const ERR: u32 = 1 << 13;
    pub const PMAOVR: u32 = 1 << 14;
    pub const CTR: u32 = 1 << 15;
}

/// `USB_FNR` bits (§37.6.3), all read only.
pub mod fnr {
    /// FN\[10:0\]: "the 11-bits frame number contained in the last received SOF packet ... updated on the
    /// generation of an SOF interrupt".
    pub const FN: u32 = 0x7ff;
    /// LSOF\[1:0\]: "written by the hardware when an ESOF interrupt is generated, counting the number of
    /// consecutive SOF packets lost. At the reception of an SOF packet, these bits are cleared."
    pub const LSOF_SHIFT: u32 = 11;
    pub const LSOF: u32 = 0b11 << LSOF_SHIFT;
    /// LCK: "set by the hardware when at least two consecutive SOF packets have been received after the end of an
    /// USB reset condition or after the end of an USB resume sequence. Once locked, the frame timer remains in this
    /// state until an USB reset or USB suspend event occurs."
    pub const LCK: u32 = 1 << 13;
}

/// `USB_DADDR` bits (§37.6.4).
pub mod daddr {
    pub const ADD: u32 = 0x7f;
    pub const EF: u32 = 1 << 7;
}

/// `USB_BCDR` bits (§37.6.6).
pub mod bcdr {
    /// DPPU_DPD: the DP pull-up in device mode, which tells the host a device is there.
    pub const DPPU: u32 = 1 << 15;
}

/// The two words of buffer descriptor table entry n (§37.7): `CHEP_TXRXBD_n` at word `2n`, `CHEP_RXTXBD_n` at
/// word `2n + 1` of USB SRAM.
pub mod bd {
    pub const ADDR: u32 = 0xffff;
    pub const COUNT_SHIFT: u32 = 16;
    pub const COUNT: u32 = 0x3ff << COUNT_SHIFT;
    /// BLSIZE and NUM_BLOCK of a receive buffer, kept when the count is reset.
    pub const RX_SIZE: u32 = 0x3f << 26;
}

/// What the driver needs from the peripheral. Every method is one 32-bit access, so an implementation is a
/// volatile load or store ([Mmio]) or a model of the hardware (`v4::sim::Sim`). The usb_v3 peripheral
/// (`crate::v3::Mmio`) has the same registers in the low 16 bits and a USB SRAM that only takes 16-bit accesses;
/// its implementation makes each word access two half-word ones.
pub trait Regs {
    /// Bytes of USB SRAM: [Driver](super::Driver)`::new` fails the build when the device's layout needs more.
    const SRAM_SIZE: usize;
    fn chep(&self, n: usize) -> u32;
    fn set_chep(&self, n: usize, value: u32);
    fn cntr(&self) -> u32;
    fn set_cntr(&self, value: u32);
    fn istr(&self) -> u32;
    fn set_istr(&self, value: u32);
    fn fnr(&self) -> u32;
    fn daddr(&self) -> u32;
    fn set_daddr(&self, value: u32);
    fn bcdr(&self) -> u32;
    fn set_bcdr(&self, value: u32);
    /// Word `index` of USB SRAM (byte offset `4 * index`): the access scheme is 32 bits (R11, RM0444 Table 225).
    fn sram(&self, index: usize) -> u32;
    fn set_sram(&self, index: usize, value: u32);
    /// `USB_BTABLE`, the buffer table's offset in USB SRAM, on peripherals that have it (usb_v3; usb_v4 has the
    /// table at 0). The driver writes 0.
    fn set_btable(&self, _value: u32) {}
    /// Waits at least `ns` nanoseconds: tSTARTUP (R9) and the ES0548 2.16.1 wait before reading an OUT packet (R6).
    fn delay_ns(&self, ns: u32);
}

/// The peripheral's registers in memory, with the core clock for [Regs::delay_ns].
#[derive(Copy, Clone, Debug)]
pub struct Mmio {
    usb: usize,
    sram: usize,
    /// The core clock in MHz, rounded up.
    mhz: u32,
}

impl Mmio {
    /// USB at 0x4000_5C00 and USB SRAM at 0x4000_9800 (RM0444 Rev 6 §2.2, Table 6 "Peripheral register boundary
    /// addresses"): STM32G0B1 / G0C1.
    pub const G0_USB: usize = 0x4000_5c00;
    pub const G0_SRAM: usize = 0x4000_9800;

    /// The G0's peripheral, `hclk_hz` the core clock the delays count in.
    ///
    /// # Safety
    /// The caller owns the USB peripheral: nothing else writes its registers or its SRAM.
    pub const unsafe fn g0(hclk_hz: u32) -> Self {
        unsafe { Self::new(Self::G0_USB, Self::G0_SRAM, hclk_hz) }
    }

    /// A peripheral of the same kind at other addresses (the H5, U0, U3 and C0 parts with `usbram_32_*`).
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
}

impl Regs for Mmio {
    /// "Dedicated packet buffer memory SRAM of 2048 bytes" (RM0444 Table 225).
    const SRAM_SIZE: usize = crate::layout::SRAM_SIZE;
    #[inline(always)]
    fn chep(&self, n: usize) -> u32 {
        unsafe { self.reg(offset::CHEP0R + 4 * n).read_volatile() }
    }
    #[inline(always)]
    fn set_chep(&self, n: usize, value: u32) {
        unsafe { self.reg(offset::CHEP0R + 4 * n).write_volatile(value) }
    }
    #[inline(always)]
    fn cntr(&self) -> u32 {
        unsafe { self.reg(offset::CNTR).read_volatile() }
    }
    #[inline(always)]
    fn set_cntr(&self, value: u32) {
        unsafe { self.reg(offset::CNTR).write_volatile(value) }
    }
    #[inline(always)]
    fn istr(&self) -> u32 {
        unsafe { self.reg(offset::ISTR).read_volatile() }
    }
    #[inline(always)]
    fn set_istr(&self, value: u32) {
        unsafe { self.reg(offset::ISTR).write_volatile(value) }
    }
    #[inline(always)]
    fn fnr(&self) -> u32 {
        unsafe { self.reg(offset::FNR).read_volatile() }
    }
    #[inline(always)]
    fn daddr(&self) -> u32 {
        unsafe { self.reg(offset::DADDR).read_volatile() }
    }
    #[inline(always)]
    fn set_daddr(&self, value: u32) {
        unsafe { self.reg(offset::DADDR).write_volatile(value) }
    }
    #[inline(always)]
    fn bcdr(&self) -> u32 {
        unsafe { self.reg(offset::BCDR).read_volatile() }
    }
    #[inline(always)]
    fn set_bcdr(&self, value: u32) {
        unsafe { self.reg(offset::BCDR).write_volatile(value) }
    }
    #[inline(always)]
    fn sram(&self, index: usize) -> u32 {
        unsafe { ((self.sram + 4 * index) as *const u32).read_volatile() }
    }
    #[inline(always)]
    fn set_sram(&self, index: usize, value: u32) {
        unsafe { ((self.sram + 4 * index) as *mut u32).write_volatile(value) }
    }
    fn delay_ns(&self, ns: u32) {
        delay_ns(ns, self.mhz);
    }
}

/// At least `ns` nanoseconds in cycles of a `mhz` clock: `ns * mhz / 1000`, rounded up, without a division. The
/// Cortex-M0+ has no divider, and a 64-bit one (`__aeabi_uldivmod`) cost about 1 KiB of flash and its run time on
/// every OUT packet (the ES0548 2.16.1 wait). 1049 / 2^20 is a little more than 1 / 1000, so the wait is never
/// shorter. Holds for `ns * mhz` below 4 000 000 (the waits here are 1 µs at most, clocks 550 MHz at most).
pub(crate) const fn cycles(ns: u32, mhz: u32) -> u32 {
    debug_assert!(ns as u64 * mhz as u64 <= 4_000_000);
    ((ns * mhz * 1049) >> 20) + 1
}

/// At least `ns` nanoseconds of a `mhz` core clock.
#[inline(always)]
pub(crate) fn delay_ns(ns: u32, mhz: u32) {
    delay_cycles(cycles(ns, mhz));
}

/// At least `cycles` core cycles.
#[inline(always)]
fn delay_cycles(cycles: u32) {
    #[cfg(all(target_arch = "arm", target_os = "none"))]
    cortex_m::asm::delay(cycles);
    #[cfg(not(all(target_arch = "arm", target_os = "none")))]
    for _ in 0..cycles {
        core::hint::spin_loop();
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn cycles_never_short() {
        for mhz in [1, 8, 16, 48, 64, 170, 250, 480, 550] {
            for ns in [1, 100, 800, 1000, 7000] {
                if ns * mhz > 4_000_000 {
                    continue;
                }
                let exact = (ns as u64 * mhz as u64).div_ceil(1000) as u32;
                let c = super::cycles(ns, mhz);
                assert!(
                    c >= exact && c <= exact + 1 + exact / 100,
                    "{ns} ns at {mhz} MHz: {c}, exact {exact}"
                );
            }
        }
    }
}
