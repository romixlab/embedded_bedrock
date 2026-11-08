//! The registers of the Synopsys OTG core the device driver touches, their bits, and the [Regs] trait that reaches
//! them: [Mmio] on the chip, the simulated core (`otg::sim`, feature `sim`) in host tests.
//!
//! Offsets and bits are RM0468 Rev 3 §62.14; `tests/otg_regs.rs` holds every one of them to
//! embassy-usb-synopsys-otg's `otg_v1` register block, which is generated from the same core.

/// Register offsets from the core's base.
pub mod offset {
    pub const GOTGCTL: usize = 0x000;
    pub const GAHBCFG: usize = 0x008;
    pub const GUSBCFG: usize = 0x00c;
    pub const GRSTCTL: usize = 0x010;
    pub const GINTSTS: usize = 0x014;
    pub const GINTMSK: usize = 0x018;
    /// Status read and pop (§62.14.10): a read pops the top entry of the receive FIFO.
    pub const GRXSTSP: usize = 0x020;
    pub const GRXFSIZ: usize = 0x024;
    pub const DIEPTXF0: usize = 0x028;
    pub const GCCFG: usize = 0x038;
    /// `OTG_DIEPTXFx`: "Address offset: 0x104 + 0x04 * (x - 1), (x = 1 to 8)" (§62.14.19).
    pub const fn dieptxf(x: usize) -> usize {
        if x == 0 {
            DIEPTXF0
        } else {
            0x104 + 4 * (x - 1)
        }
    }
    pub const DCFG: usize = 0x800;
    pub const DCTL: usize = 0x804;
    pub const DSTS: usize = 0x808;
    pub const DIEPMSK: usize = 0x810;
    pub const DOEPMSK: usize = 0x814;
    pub const DAINT: usize = 0x818;
    pub const DAINTMSK: usize = 0x81c;
    /// "Address offset: 0x900 + 0x20 * x, (x = 0 to 8)" (§62.14.54).
    pub const fn diepctl(x: usize) -> usize {
        0x900 + 0x20 * x
    }
    pub const fn diepint(x: usize) -> usize {
        0x908 + 0x20 * x
    }
    pub const fn dieptsiz(x: usize) -> usize {
        0x910 + 0x20 * x
    }
    pub const fn dtxfsts(x: usize) -> usize {
        0x918 + 0x20 * x
    }
    pub const fn doepctl(x: usize) -> usize {
        0xb00 + 0x20 * x
    }
    pub const fn doepint(x: usize) -> usize {
        0xb08 + 0x20 * x
    }
    pub const fn doeptsiz(x: usize) -> usize {
        0xb10 + 0x20 * x
    }
    pub const PCGCCTL: usize = 0xe00;
    /// The data FIFO of endpoint x: pushes go to its transmit FIFO, pops (from any of them) come from the receive
    /// FIFO; 0x1000 bytes each from offset 0x1000.
    pub const fn fifo(x: usize) -> usize {
        0x1000 * (x + 1)
    }
}

/// `OTG_GOTGCTL` (§62.14.1).
pub mod gotgctl {
    /// BVALOEN: the B-session valid override is on.
    pub const BVALOEN: u32 = 1 << 6;
    /// BVALOVAL: the B-session valid value the override gives.
    pub const BVALOVAL: u32 = 1 << 7;
}

/// `OTG_GAHBCFG` (§62.14.3).
pub mod gahbcfg {
    /// GINTMSK: "Unmask the interrupt assertion to the application".
    pub const GINT: u32 = 1 << 0;
}

/// `OTG_GUSBCFG` (§62.14.4).
pub mod gusbcfg {
    pub const PHYSEL: u32 = 1 << 6;
    pub const TRDT_SHIFT: u32 = 10;
    pub const TRDT: u32 = 0xf << TRDT_SHIFT;
    pub const ULPIFSLS: u32 = 1 << 17;
    pub const ULPICSM: u32 = 1 << 19;
    pub const ULPIEVBUSD: u32 = 1 << 20;
    pub const ULPIEVBUSI: u32 = 1 << 21;
    pub const FHMOD: u32 = 1 << 29;
    pub const FDMOD: u32 = 1 << 30;
}

/// `OTG_GRSTCTL` (§62.14.5).
pub mod grstctl {
    pub const CSRST: u32 = 1 << 0;
    pub const RXFFLSH: u32 = 1 << 4;
    pub const TXFFLSH: u32 = 1 << 5;
    pub const TXFNUM_SHIFT: u32 = 6;
    /// TXFNUM "10000: Flush all the transmit FIFOs in device or host mode".
    pub const TXFNUM_ALL: u32 = 0x10;
    pub const AHBIDL: u32 = 1 << 31;
}

/// `OTG_GINTSTS` / `OTG_GINTMSK` (§62.14.6, §62.14.7). RXFLVL, IEPINT and OEPINT are read only (they follow the
/// receive FIFO and `OTG_DAINT`); the others the driver uses are cleared by writing 1.
pub mod gint {
    pub const RXFLVL: u32 = 1 << 4;
    pub const GONAKEFF: u32 = 1 << 7;
    pub const ESUSP: u32 = 1 << 10;
    pub const USBSUSP: u32 = 1 << 11;
    pub const USBRST: u32 = 1 << 12;
    pub const ENUMDNE: u32 = 1 << 13;
    pub const IEPINT: u32 = 1 << 18;
    pub const OEPINT: u32 = 1 << 19;
    pub const WKUPINT: u32 = 1 << 31;
}

/// `OTG_GRXSTSP` in device mode (§62.14.10).
pub mod grxsts {
    pub const EPNUM: u32 = 0xf;
    pub const BCNT_SHIFT: u32 = 4;
    pub const BCNT: u32 = 0x7ff << BCNT_SHIFT;
    pub const PKTSTS_SHIFT: u32 = 17;
    pub const PKTSTS: u32 = 0xf << PKTSTS_SHIFT;
    /// PKTSTS values: "0001: Global OUT NAK (triggers an interrupt) 0010: OUT data packet received 0011: OUT
    /// transfer completed (triggers an interrupt) 0100: SETUP transaction completed (triggers an interrupt) 0110:
    /// SETUP data packet received".
    pub const GLOBAL_OUT_NAK: u32 = 0b0001;
    pub const OUT_DATA: u32 = 0b0010;
    pub const OUT_DONE: u32 = 0b0011;
    pub const SETUP_DONE: u32 = 0b0100;
    pub const SETUP_DATA: u32 = 0b0110;
}

/// The FIFO size registers `OTG_GRXFSIZ`, `OTG_DIEPTXF0`, `OTG_DIEPTXFx`: depth in words in bits 31:16 (16:0 for
/// GRXFSIZ, which has no start), start address in words in bits 15:0.
pub mod fsiz {
    pub const fn value(start: u16, depth: u16) -> u32 {
        (depth as u32) << 16 | start as u32
    }
}

/// `OTG_GCCFG` (§62.14.15).
pub mod gccfg {
    /// PWRDWN: "0 = USB FS PHY disabled 1 = USB FS PHY enabled".
    pub const PWRDWN: u32 = 1 << 16;
    pub const VBDEN: u32 = 1 << 21;
}

/// `OTG_DCFG` (§62.14.39).
pub mod dcfg {
    /// DSPD: "00: High speed 01: Full speed using HS 10: Reserved 11: Full speed using internal FS PHY".
    pub const DSPD: u32 = 0b11;
    pub const DSPD_HIGH: u32 = 0b00;
    pub const DSPD_FULL_ULPI: u32 = 0b01;
    pub const DSPD_FULL_INTERNAL: u32 = 0b11;
    pub const DAD_SHIFT: u32 = 4;
    pub const DAD: u32 = 0x7f << DAD_SHIFT;
    /// PFIVL "00: 80% of the frame interval".
    pub const PFIVL: u32 = 0b11 << 11;
    pub const XCVRDLY: u32 = 1 << 14;
}

/// `OTG_DCTL` (§62.14.40).
pub mod dctl {
    pub const SDIS: u32 = 1 << 1;
    pub const SGONAK: u32 = 1 << 9;
    pub const CGONAK: u32 = 1 << 10;
}

/// `OTG_DSTS` (§62.14.41).
pub mod dsts {
    pub const SUSPSTS: u32 = 1 << 0;
    pub const ENUMSPD_SHIFT: u32 = 1;
    /// ENUMSPD: "00: High Speed using HS PHY 01: Full Speed using HS PHY 11: Full speed using embedded FS PHY".
    pub const ENUMSPD: u32 = 0b11 << ENUMSPD_SHIFT;
    pub const ENUMSPD_HIGH: u32 = 0b00;
}

/// `OTG_DAINT` / `OTG_DAINTMSK`: IN endpoint n in bit n, OUT endpoint n in bit 16 + n.
pub mod daint {
    pub const fn in_ep(n: usize) -> u32 {
        1 << n
    }
    pub const fn out_ep(n: usize) -> u32 {
        1 << (16 + n)
    }
}

/// `OTG_DIEPCTLx` and `OTG_DOEPCTLx` (§62.14.54, §62.14.60, §62.14.64): the same bits in both, TXFNUM only for IN.
///
/// EPENA, EPDIS, SD0PID, SD1PID, SNAK and CNAK are actions: a write of 1 does something, a write of 0 nothing. A
/// write leaves them 0 unless it means them and never writes back what it read: EPENA reads 1 while the endpoint
/// is enabled and "The core clears this bit before setting any of the following interrupts", so writing back a 1
/// read before the core cleared it would enable the endpoint again. [ctl::KEEP] are the bits a write keeps.
pub mod ctl {
    pub const MPSIZ: u32 = 0x7ff;
    /// EP0's MPSIZ (§62.14.60): "00: 64 bytes 01: 32 bytes 10: 16 bytes 11: 8 bytes".
    pub const fn mpsiz0(max_packet: u8) -> u32 {
        match max_packet {
            8 => 0b11,
            16 => 0b10,
            32 => 0b01,
            _ => 0b00,
        }
    }
    pub const USBAEP: u32 = 1 << 15;
    /// DPID: the data PID of the next packet, 1 for DATA1 (interrupt and bulk).
    pub const DPID: u32 = 1 << 16;
    pub const NAKSTS: u32 = 1 << 17;
    pub const EPTYP_SHIFT: u32 = 18;
    pub const EPTYP: u32 = 0b11 << EPTYP_SHIFT;
    /// EPTYP: "00: Control 01: Isochronous 10: Bulk 11: Interrupt".
    pub const EPTYP_BULK: u32 = 0b10 << EPTYP_SHIFT;
    pub const EPTYP_INTERRUPT: u32 = 0b11 << EPTYP_SHIFT;
    pub const STALL: u32 = 1 << 21;
    pub const TXFNUM_SHIFT: u32 = 22;
    pub const TXFNUM: u32 = 0xf << TXFNUM_SHIFT;
    pub const CNAK: u32 = 1 << 26;
    pub const SNAK: u32 = 1 << 27;
    pub const SD0PID: u32 = 1 << 28;
    pub const EPDIS: u32 = 1 << 30;
    pub const EPENA: u32 = 1 << 31;
    /// The read-write configuration a write carries over from the value read.
    pub const KEEP: u32 = MPSIZ | USBAEP | EPTYP | STALL | TXFNUM;
}

/// `OTG_DIEPINTx` / `OTG_DOEPINTx` and their common masks (§62.14.55, §62.14.61), each flag cleared by writing 1.
pub mod epint {
    pub const XFRC: u32 = 1 << 0;
    pub const EPDISD: u32 = 1 << 1;
    /// IN: TOC, timeout condition. OUT: STUP, SETUP phase done.
    pub const TOC: u32 = 1 << 3;
    pub const STUP: u32 = 1 << 3;
    /// IN: INEPNE, "IN endpoint NAK effective".
    pub const INEPNE: u32 = 1 << 6;
}

/// `OTG_DIEPTSIZx` / `OTG_DOEPTSIZx` (§62.14.56, §62.14.59, §62.14.62, §62.14.65). Endpoint 0 has the narrow
/// fields: XFRSIZ 6:0, PKTCNT 20:19 (IN) or bit 19 (OUT); the others XFRSIZ 18:0 and PKTCNT 28:19.
pub mod tsiz {
    pub const XFRSIZ: u32 = 0x7_ffff;
    pub const PKTCNT_SHIFT: u32 = 19;
    pub const PKTCNT: u32 = 0x3ff << PKTCNT_SHIFT;
    /// STUPCNT of `OTG_DOEPTSIZ0`, bits 30:29: "11: 3 packets".
    pub const STUPCNT_SHIFT: u32 = 29;
    pub const STUPCNT: u32 = 0b11 << STUPCNT_SHIFT;
    /// One packet of `len` bytes.
    pub const fn one(len: usize) -> u32 {
        1 << PKTCNT_SHIFT | len as u32
    }
}

/// What the driver needs from the core. Every method is one 32-bit access (or a wait), so an implementation is a
/// volatile load or store ([Mmio]) or a model of the hardware (`otg::sim::Sim`).
pub trait Regs {
    fn read(&self, offset: usize) -> u32;
    fn write(&self, offset: usize, value: u32);
    /// Pops one word of received data from the receive FIFO.
    fn pop(&self) -> u32 {
        self.read(offset::fifo(0))
    }
    /// Pushes one word into the transmit FIFO of IN endpoint `n`.
    fn push(&self, n: usize, word: u32) {
        self.write(offset::fifo(n), word)
    }
    /// Waits at least `us` microseconds: the 25 ms after FDMOD.
    fn delay_us(&self, us: u32);
}

/// The core's registers in memory, with the core clock for [Regs::delay_us].
#[derive(Copy, Clone, Debug)]
pub struct Mmio {
    base: usize,
    /// The core clock in MHz, rounded up.
    mhz: u32,
}

impl Mmio {
    /// USB1 OTG_HS at 0x4004_0000 (RM0468 Rev 3 §2.3.2, Table 7: "0x40040000 - 0x4007FFFF USB1 OTG_HS"):
    /// STM32H723 / H725 / H730 / H733 / H735.
    pub const H7_OTG_HS: usize = 0x4004_0000;

    /// The core at `base`, `hclk_hz` the core clock the waits count in.
    ///
    /// # Safety
    /// `base` is an OTG core's register block and the caller owns it: nothing else writes its registers or FIFOs.
    pub const unsafe fn new(base: usize, hclk_hz: u32) -> Self {
        Mmio {
            base,
            mhz: hclk_hz.div_ceil(1_000_000),
        }
    }
}

impl Regs for Mmio {
    #[inline(always)]
    fn read(&self, offset: usize) -> u32 {
        unsafe { ((self.base + offset) as *const u32).read_volatile() }
    }
    #[inline(always)]
    fn write(&self, offset: usize, value: u32) {
        unsafe { ((self.base + offset) as *mut u32).write_volatile(value) }
    }
    fn delay_us(&self, us: u32) {
        #[cfg(all(target_arch = "arm", target_os = "none"))]
        cortex_m::asm::delay(us.saturating_mul(self.mhz));
        #[cfg(not(all(target_arch = "arm", target_os = "none")))]
        for _ in 0..us.saturating_mul(self.mhz) {
            core::hint::spin_loop();
        }
    }
}
