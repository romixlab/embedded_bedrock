//! Register and USB SRAM access for the peripherals with 16-bit registers and 16-bit USB SRAM: usb_v3
//! ([crate::v3]), usb_v1 and usb_v2 ([crate::v1]). Their `Regs` implementations differ only in the SRAM's
//! spacing, its size and the DP pull-up; the rest is [Bus16] and the [regs16] macro.
//!
//! - Registers: usb_v4's low 16 bits under other names, the upper 16 "Reserved". A write keeps to the low half, so
//!   usb_v4's host-mode `rc_w0` bits never reach them.
//! - USB SRAM: half-words only. The driver's 32-bit word `i` (bytes `4i` to `4i + 3` as the peripheral addresses
//!   them) is half-word `2i` (low) and `2i + 1` (high), at `stride` bytes per half-word in the CPU's address space:
//!   2 for the "2 x 16 bits / word" access scheme, 4 for "1 x 16 bits / word" (RM0316 Rev 10 Table 178).

/// The bits of a 16-bit peripheral's register: the low half-word.
const REG_MASK: u32 = 0xffff;

/// A 16-bit peripheral's register block and USB SRAM in memory, with the core clock for the short waits.
#[derive(Copy, Clone, Debug)]
pub(crate) struct Bus16 {
    usb: usize,
    sram: usize,
    /// The core clock in MHz, rounded up.
    mhz: u32,
}

impl Bus16 {
    /// # Safety
    /// `usb` and `sram` are the peripheral's register block and its USB SRAM, and the caller owns them.
    pub(crate) const unsafe fn new(usb: usize, sram: usize, hclk_hz: u32) -> Self {
        Bus16 {
            usb,
            sram,
            mhz: hclk_hz.div_ceil(1_000_000),
        }
    }

    pub(crate) const fn usb(&self) -> usize {
        self.usb
    }

    pub(crate) const fn sram_base(&self) -> usize {
        self.sram
    }

    #[inline(always)]
    pub(crate) fn read(&self, offset: usize) -> u32 {
        // Safety: the register block is ours (Bus16::new)
        unsafe { ((self.usb + offset) as *const u32).read_volatile() }
    }

    #[inline(always)]
    pub(crate) fn write(&self, offset: usize, value: u32) {
        // Safety: as in read
        unsafe { ((self.usb + offset) as *mut u32).write_volatile(value & REG_MASK) }
    }

    #[inline(always)]
    fn half(&self, stride: usize, index: usize) -> *mut u16 {
        (self.sram + stride * index) as *mut u16
    }

    /// The driver's word `index`: half-words `2 * index` (low) and `2 * index + 1` (high), `stride` bytes apart.
    #[inline(always)]
    pub(crate) fn sram(&self, stride: usize, index: usize) -> u32 {
        // Safety: the USB SRAM is ours (Bus16::new); the driver keeps `index` inside the layout
        unsafe {
            let lo = self.half(stride, 2 * index).read_volatile() as u32;
            let hi = self.half(stride, 2 * index + 1).read_volatile() as u32;
            lo | (hi << 16)
        }
    }

    #[inline(always)]
    pub(crate) fn set_sram(&self, stride: usize, index: usize, value: u32) {
        // Safety: as in sram
        unsafe {
            self.half(stride, 2 * index).write_volatile(value as u16);
            self.half(stride, 2 * index + 1)
                .write_volatile((value >> 16) as u16);
        }
    }

    pub(crate) fn delay_ns(&self, ns: u32) {
        crate::v4::regs::delay_ns(ns, self.mhz);
    }
}

/// The `Regs` methods every 16-bit peripheral shares, on the [Bus16] at `self.<path>`, its SRAM `$stride` bytes
/// per half-word. `SRAM_SIZE` and `set_pull_up` are the implementation's own.
macro_rules! regs16 {
    ($($bus:ident).+, $stride:expr) => {
        #[inline(always)]
        fn chep(&self, n: usize) -> u32 {
            self.$($bus).+.read($crate::v4::regs::offset::CHEP0R + 4 * n)
        }
        #[inline(always)]
        fn set_chep(&self, n: usize, value: u32) {
            self.$($bus).+.write($crate::v4::regs::offset::CHEP0R + 4 * n, value)
        }
        #[inline(always)]
        fn cntr(&self) -> u32 {
            self.$($bus).+.read($crate::v4::regs::offset::CNTR)
        }
        #[inline(always)]
        fn set_cntr(&self, value: u32) {
            self.$($bus).+.write($crate::v4::regs::offset::CNTR, value)
        }
        #[inline(always)]
        fn istr(&self) -> u32 {
            self.$($bus).+.read($crate::v4::regs::offset::ISTR)
        }
        #[inline(always)]
        fn set_istr(&self, value: u32) {
            self.$($bus).+.write($crate::v4::regs::offset::ISTR, value)
        }
        #[inline(always)]
        fn fnr(&self) -> u32 {
            self.$($bus).+.read($crate::v4::regs::offset::FNR)
        }
        #[inline(always)]
        fn daddr(&self) -> u32 {
            self.$($bus).+.read($crate::v4::regs::offset::DADDR)
        }
        #[inline(always)]
        fn set_daddr(&self, value: u32) {
            self.$($bus).+.write($crate::v4::regs::offset::DADDR, value)
        }
        #[inline(always)]
        fn sram(&self, index: usize) -> u32 {
            self.$($bus).+.sram($stride, index)
        }
        #[inline(always)]
        fn set_sram(&self, index: usize, value: u32) {
            self.$($bus).+.set_sram($stride, index, value)
        }
        /// `USB_BTABLE` (offset 0x50 on usb_v1, v2 and v3).
        #[inline(always)]
        fn set_btable(&self, value: u32) {
            self.$($bus).+.write($crate::v3::offset::BTABLE, value)
        }
        fn delay_ns(&self, ns: u32) {
            self.$($bus).+.delay_ns(ns);
        }
    };
}
pub(crate) use regs16;
