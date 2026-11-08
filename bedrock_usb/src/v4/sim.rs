//! A simulated usb_v4 peripheral for host tests: the register semantics the driver relies on and the hardware's
//! side of a transfer, so the rules become tests (the way `bedrock_clocks_rt` is tested on a simulated chip).
//!
//! Registers:
//! - `USB_CHEPnR`: `rw` bits take the value written, `rc_w0` bits (VTRX, VTTX, the host error bits) clear where 0
//!   is written and stay where 1 is (R1), toggle bits (STAT, DTOG) flip where 1 is written (R2), SETUP is read
//!   only. [Sim::race] runs a hardware event between the driver's read and its next write.
//! - `USB_ISTR`: CTR, IDN and DIR computed from the endpoint registers (the lowest register with VTRX or VTTX
//!   set), every other flag `rc_w0`.
//! - `USB_FNR`: FN of the last SOF, LSOF counting the ESOFs since it, LCK from the second SOF after a bus reset
//!   or a wake-up until the next bus reset or suspend (§37.6.3).
//! - USB SRAM as 512 words.
//!
//! The hardware side ([Sim::host_setup], [Sim::host_out], [Sim::host_in], [Sim::bus_reset], [Sim::suspend],
//! [Sim::wakeup], [Sim::sof], [Sim::esof]) follows RM0444 Rev 6 §37.5: a SETUP sets both STAT to NAK and is dropped while VTRX is pending
//! (R5), a single-buffered transfer sets STAT to NAK, a double-buffered one toggles DTOG and NAKs on a buffer
//! conflict with STAT left VALID (R4), a packet longer than its receive buffer is answered with STALL and no
//! interrupt (R7), a control endpoint with STATUS_OUT stalls an OUT with data (R5).
//!
//! What it watches: [Inner::r4] lists writes to a double-buffered register that toggle a DTOG / SW_BUF bit and a
//! STAT bit at once; [Inner::r6] lists SRAM reads less than 800 ns after an OUT or SETUP packet arrived.

extern crate std;

use core::cell::RefCell;
use std::boxed::Box;
use std::vec::Vec;

use super::chep::{Stat, stat_rx, stat_tx};
use super::regs::{Regs, bd, chep::*, fnr, istr};

/// One access the driver made, in order.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum Op {
    /// A write of `value` to `USB_CHEPnR`, with the register before and after.
    Chep {
        n: usize,
        value: u32,
        before: u32,
        after: u32,
    },
    Cntr(u32),
    Istr(u32),
    Daddr(u32),
    Bcdr(u32),
    SramWrite {
        index: usize,
        value: u32,
    },
    SramRead {
        index: usize,
    },
    Delay(u32),
}

/// The host's view of a transaction.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum Handshake {
    Ack,
    Nak,
    Stall,
    /// No answer: the endpoint is disabled, or a SETUP was dropped.
    None,
}

/// The chip's state, open to the test.
pub struct Inner {
    pub chep: [u32; 8],
    pub cntr: u32,
    /// ISTR's `rc_w0` flags; CTR, IDN and DIR are computed.
    pub istr: u32,
    pub fnr: u32,
    /// SOFs since the last bus reset or wake-up, up to 2 (LCK).
    pub sofs: u8,
    pub daddr: u32,
    pub bcdr: u32,
    pub sram: [u32; 512],
    pub log: Vec<Op>,
    /// An OUT or SETUP packet arrived and no 800 ns wait followed yet.
    pub unsettled: bool,
    /// R4 violations: (register, value written).
    pub r4: Vec<(usize, u32)>,
    /// R6 violations: SRAM word indices read while unsettled.
    pub r6: Vec<usize>,
}

type Race = Box<dyn FnOnce(&mut Inner)>;

/// The simulated peripheral. `&Sim` implements [Regs].
pub struct Sim {
    inner: RefCell<Inner>,
    race: RefCell<Option<Race>>,
}

impl Default for Sim {
    fn default() -> Self {
        Self::new()
    }
}

impl Sim {
    /// The state after a system reset: CNTR with USBRST and PDWN set, everything else 0.
    pub fn new() -> Self {
        Sim {
            inner: RefCell::new(Inner {
                chep: [0; 8],
                cntr: 0b11,
                istr: 0,
                fnr: 0,
                sofs: 0,
                daddr: 0,
                bcdr: 0,
                sram: [0; 512],
                log: Vec::new(),
                unsettled: false,
                r4: Vec::new(),
                r6: Vec::new(),
            }),
            race: RefCell::new(None),
        }
    }

    /// The state, to read or change.
    pub fn with<T>(&self, f: impl FnOnce(&mut Inner) -> T) -> T {
        f(&mut self.inner.borrow_mut())
    }

    pub fn chep_value(&self, n: usize) -> u32 {
        self.inner.borrow().chep[n]
    }

    /// Takes the log of accesses so far.
    pub fn take_log(&self) -> Vec<Op> {
        core::mem::take(&mut self.inner.borrow_mut().log)
    }

    /// Runs `f` at the driver's next write to a `USB_CHEPnR`, before the write lands: the hardware acting between
    /// the driver's read and its write.
    pub fn race(&self, f: impl FnOnce(&mut Inner) + 'static) {
        *self.race.borrow_mut() = Some(Box::new(f));
    }

    /// Whether the interrupt line is up: a flag pending whose mask is set in CNTR.
    pub fn irq(&self) -> bool {
        let i = self.inner.borrow();
        let flags = istr_value(&i);
        let m = i.cntr;
        (flags & istr::CTR != 0 && m & super::regs::cntr::CTRM != 0)
            || (flags & istr::RST_DCON != 0 && m & super::regs::cntr::RST_DCONM != 0)
            || (flags & istr::SUSP != 0 && m & super::regs::cntr::SUSPM != 0)
            || (flags & istr::WKUP != 0 && m & super::regs::cntr::WKUPM != 0)
            || (flags & istr::SOF != 0 && m & super::regs::cntr::SOFM != 0)
            || (flags & istr::ESOF != 0 && m & super::regs::cntr::ESOFM != 0)
    }

    /// A bus reset (RST_DCON): every endpoint register cleared but VTRX and VTTX (§37.6.7), the address 0 and the
    /// function disabled.
    pub fn bus_reset(&self) {
        let mut i = self.inner.borrow_mut();
        for c in i.chep.iter_mut() {
            *c &= VTRX | VTTX;
        }
        i.daddr = 0;
        i.istr |= istr::RST_DCON;
        i.sofs = 0;
        i.fnr &= !fnr::LCK;
    }

    pub fn suspend(&self) {
        let mut i = self.inner.borrow_mut();
        i.istr |= istr::SUSP;
        i.fnr &= !fnr::LCK;
    }

    pub fn wakeup(&self) {
        let mut i = self.inner.borrow_mut();
        i.istr |= istr::WKUP;
        i.sofs = 0;
    }

    /// A SOF packet with frame number `frame` (11 bits): FN updated, LSOF cleared, LCK set from the second SOF
    /// after a bus reset or wake-up, the SOF flag set.
    pub fn sof(&self, frame: u16) {
        let mut i = self.inner.borrow_mut();
        i.sofs = (i.sofs + 1).min(2);
        let lck = if i.sofs >= 2 {
            fnr::LCK
        } else {
            i.fnr & fnr::LCK
        };
        i.fnr = lck | (frame as u32 & fnr::FN);
        i.istr |= istr::SOF;
    }

    /// A SOF expected and not received: LSOF counts it (saturating at 3), the ESOF flag set.
    pub fn esof(&self) {
        let mut i = self.inner.borrow_mut();
        let lsof = ((i.fnr & fnr::LSOF) >> fnr::LSOF_SHIFT)
            .saturating_add(1)
            .min(3);
        i.fnr = (i.fnr & !fnr::LSOF) | (lsof << fnr::LSOF_SHIFT);
        i.istr |= istr::ESOF;
    }

    fn is_double(c: u32) -> bool {
        c & EPKIND != 0 && (c & UTYPE) >> UTYPE_SHIFT == 0b00
    }

    fn is_control(c: u32) -> bool {
        (c & UTYPE) >> UTYPE_SHIFT == 0b01
    }

    /// The register of endpoint `ep`: the driver puts endpoint n in register n, EA n.
    fn reg_of(i: &Inner, ep: u8) -> Option<usize> {
        let n = ep as usize;
        (n < 8 && i.chep[n] & EA == ep as u32).then_some(n)
    }

    /// A SETUP transaction to endpoint `ep` (R5): taken when STATRX is VALID or STALL ("If the control endpoint
    /// is defined as STALL in the receive direction, then the SETUP packet is accepted anyway"), not answered
    /// when NAK or DISABLED or while VTRX is still set. Taken: data in the receive buffer, COUNT 8, SETUP and VTRX
    /// set, DTOGRX and DTOGTX 1 (the SETUP is DATA0 and its ACK moves the receive toggle on), both STAT NAK.
    pub fn host_setup(&self, ep: u8, data: [u8; 8]) -> Handshake {
        let mut i = self.inner.borrow_mut();
        let Some(n) = Self::reg_of(&i, ep) else {
            return Handshake::None;
        };
        let c = i.chep[n];
        if !Self::is_control(c) || c & VTRX != 0 {
            return Handshake::None;
        }
        match stat_rx(c) {
            Stat::Disabled | Stat::Nak => return Handshake::None,
            Stat::Stall | Stat::Valid => {}
        }
        let word = i.sram[2 * n + 1];
        Self::store(&mut i, word, &data);
        i.sram[2 * n + 1] = (word & !bd::COUNT) | (8 << bd::COUNT_SHIFT);
        let c = (c & !(STATRX | STATTX))
            | SETUP
            | VTRX
            | DTOGRX
            | DTOGTX
            | (Stat::Nak as u32) << STATRX_SHIFT;
        i.chep[n] = c | (Stat::Nak as u32) << STATTX_SHIFT;
        i.unsettled = true;
        Handshake::Ack
    }

    fn rx_size(word: u32) -> usize {
        let blocks = ((word >> 26) & 0x1f) as usize;
        if word & (1 << 31) != 0 {
            (blocks + 1) * 32
        } else {
            blocks * 2
        }
    }

    fn store(i: &mut Inner, word: u32, data: &[u8]) {
        let base = (word & bd::ADDR) as usize / 4;
        for (k, chunk) in data.chunks(4).enumerate() {
            let mut w = [0u8; 4];
            w[..chunk.len()].copy_from_slice(chunk);
            i.sram[base + k] = u32::from_le_bytes(w);
        }
    }

    fn load(i: &Inner, word: u32) -> Vec<u8> {
        let base = (word & bd::ADDR) as usize / 4;
        let len = ((word & bd::COUNT) >> bd::COUNT_SHIFT) as usize;
        let mut v = Vec::with_capacity(len);
        for k in 0..len {
            v.push(i.sram[base + k / 4].to_le_bytes()[k % 4]);
        }
        v
    }

    /// An OUT transaction with `data` to endpoint `ep`.
    pub fn host_out(&self, ep: u8, data: &[u8]) -> Handshake {
        let mut i = self.inner.borrow_mut();
        let Some(n) = Self::reg_of(&i, ep) else {
            return Handshake::None;
        };
        let c = i.chep[n];
        match stat_rx(c) {
            Stat::Disabled => return Handshake::None,
            Stat::Stall => return Handshake::Stall,
            Stat::Nak => return Handshake::Nak,
            Stat::Valid => {}
        }
        // STATUS_OUT: "the only packet the endpoint takes is a zero-length one" (Table 236)
        if Self::is_control(c) && c & EPKIND != 0 && !data.is_empty() {
            return Handshake::Stall;
        }
        let double = Self::is_double(c);
        let word_index = if double {
            if (c & DTOGRX != 0) == (c & DTOGTX != 0) {
                return Handshake::Nak; // buffer conflict, STAT left VALID
            }
            2 * n + (c & DTOGRX != 0) as usize
        } else {
            2 * n + 1
        };
        let word = i.sram[word_index];
        if data.len() > Self::rx_size(word) {
            return Handshake::Stall; // R7: overrun, no interrupt
        }
        Self::store(&mut i, word, data);
        i.sram[word_index] = (word & !bd::COUNT) | ((data.len() as u32) << bd::COUNT_SHIFT);
        let mut c = (c ^ DTOGRX) | VTRX;
        if c & VTRX != 0 && i.chep[n] & VTRX == 0 {
            c &= !SETUP;
        }
        if !double {
            c = (c & !STATRX) | (Stat::Nak as u32) << STATRX_SHIFT;
        }
        i.chep[n] = c;
        i.unsettled = true;
        Handshake::Ack
    }

    /// An IN transaction to endpoint `ep`: the packet, or the handshake that came instead.
    pub fn host_in(&self, ep: u8) -> Result<Vec<u8>, Handshake> {
        let mut i = self.inner.borrow_mut();
        let Some(n) = Self::reg_of(&i, ep) else {
            return Err(Handshake::None);
        };
        let c = i.chep[n];
        match stat_tx(c) {
            Stat::Disabled => return Err(Handshake::None),
            Stat::Stall => return Err(Handshake::Stall),
            Stat::Nak => return Err(Handshake::Nak),
            Stat::Valid => {}
        }
        let double = Self::is_double(c);
        let word_index = if double {
            if (c & DTOGTX != 0) == (c & DTOGRX != 0) {
                return Err(Handshake::Nak);
            }
            2 * n + (c & DTOGTX != 0) as usize
        } else {
            2 * n
        };
        let data = Self::load(&i, i.sram[word_index]);
        let mut c = (c ^ DTOGTX) | VTTX;
        if !double {
            c = (c & !STATTX) | (Stat::Nak as u32) << STATTX_SHIFT;
        }
        i.chep[n] = c;
        Ok(data)
    }
}

fn istr_value(i: &Inner) -> u32 {
    let mut v = i.istr;
    if let Some(n) = (0..8).find(|&n| i.chep[n] & (VTRX | VTTX) != 0) {
        v |= istr::CTR | n as u32;
        if i.chep[n] & VTRX != 0 {
            v |= istr::DIR;
        }
    }
    v
}

impl Regs for &Sim {
    const SRAM_SIZE: usize = 4 * 512;

    fn chep(&self, n: usize) -> u32 {
        self.inner.borrow().chep[n]
    }

    fn set_chep(&self, n: usize, value: u32) {
        if let Some(f) = self.race.borrow_mut().take() {
            f(&mut self.inner.borrow_mut());
        }
        let mut i = self.inner.borrow_mut();
        let before = i.chep[n];
        if Sim::is_double(before)
            && value & (DTOGRX | DTOGTX) != 0
            && value & (STATRX | STATTX) != 0
        {
            i.r4.push((n, value));
        }
        let rw = EA | EPKIND | UTYPE | (0x7f << 16) | (1 << 24);
        let after = (value & rw)
            | (before & SETUP)
            | (before & value & RC_W0)
            | ((before ^ value) & TOGGLE);
        i.chep[n] = after;
        i.log.push(Op::Chep {
            n,
            value,
            before,
            after,
        });
    }

    fn cntr(&self) -> u32 {
        self.inner.borrow().cntr
    }

    fn set_cntr(&self, value: u32) {
        let mut i = self.inner.borrow_mut();
        i.cntr = value;
        i.log.push(Op::Cntr(value));
    }

    fn istr(&self) -> u32 {
        istr_value(&self.inner.borrow())
    }

    fn set_istr(&self, value: u32) {
        let mut i = self.inner.borrow_mut();
        i.istr &= value;
        i.log.push(Op::Istr(value));
    }

    fn fnr(&self) -> u32 {
        self.inner.borrow().fnr
    }

    fn daddr(&self) -> u32 {
        self.inner.borrow().daddr
    }

    fn set_daddr(&self, value: u32) {
        let mut i = self.inner.borrow_mut();
        i.daddr = value;
        i.log.push(Op::Daddr(value));
    }

    fn bcdr(&self) -> u32 {
        self.inner.borrow().bcdr
    }

    fn set_bcdr(&self, value: u32) {
        let mut i = self.inner.borrow_mut();
        i.bcdr = value;
        i.log.push(Op::Bcdr(value));
    }

    fn sram(&self, index: usize) -> u32 {
        let mut i = self.inner.borrow_mut();
        if i.unsettled {
            i.r6.push(index);
        }
        i.log.push(Op::SramRead { index });
        i.sram[index]
    }

    fn set_sram(&self, index: usize, value: u32) {
        let mut i = self.inner.borrow_mut();
        i.sram[index] = value;
        i.log.push(Op::SramWrite { index, value });
    }

    fn delay_ns(&self, ns: u32) {
        let mut i = self.inner.borrow_mut();
        if ns >= super::OUT_PACKET_DELAY_NS {
            i.unsettled = false;
        }
        i.log.push(Op::Delay(ns));
    }
}
