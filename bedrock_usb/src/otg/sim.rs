//! A simulated Synopsys OTG core in device mode for host tests: the register semantics the driver relies on and
//! the host's side of a transfer, after RM0468 Rev 3 §62.
//!
//! Registers:
//! - `OTG_DxEPCTLx`: the configuration bits take the value written; EPENA, EPDIS, SNAK, CNAK, SD0PID, SD1PID are
//!   actions. EPDIS on an enabled endpoint clears EPENA and raises EPDISD, SNAK sets NAKSTS (and INEPNE on an IN
//!   endpoint), at once. EP0's USBAEP reads 1, and its STALL is only set by a write ("The application can only set
//!   this bit, and the core clears it, when a SETUP token is received").
//! - `OTG_GINTSTS`, `OTG_DxEPINTx`: flags cleared by writing 1. RXFLVL, IEPINT and OEPINT are computed from the
//!   receive FIFO and `OTG_DAINT`, which is computed from the endpoint flags and `OTG_DIEPMSK` / `OTG_DOEPMSK`.
//! - `OTG_GRSTCTL`: the core soft reset and the FIFO flushes complete at once, AHBIDL reads 1.
//! - The receive FIFO as status entries with their data words, limited to GRXFSIZ; the transmit FIFOs as words,
//!   limited to their `OTG_DIEPTXFx` depth.
//!
//! The host side ([Sim::host_setup], [Sim::host_out], [Sim::host_in], [Sim::bus_reset], [Sim::enum_done],
//! [Sim::suspend], [Sim::resume]) follows §62.15.6:
//! - A SETUP is taken whatever EPENA and NAK say, sets NAK on both directions of EP0 and clears their STALL; its
//!   setup-done entry is queued at the host's next token to EP0, as on the core ("This indicates that the SETUP
//!   stage for the specified endpoint is completed and the data stage has started").
//! - An OUT packet is taken while the endpoint is enabled without NAK and the receive FIFO has room; the last
//!   packet of a transfer queues the transfer-done entry and NAKs further packets. Popping a setup-done or
//!   transfer-done entry clears EPENA and raises STUP or XFRC.
//! - An IN token sends the next packet from the endpoint's transmit FIFO while it is enabled without NAK; the
//!   last packet of a transfer clears EPENA and raises XFRC. A packet whose words aren't all in the FIFO yet is
//!   answered with NAK.
//! - The global OUT NAK: SGONAK queues the pattern and NAKs OUT data from then on, popping the pattern gives
//!   GONAKEFF, CGONAK ends both.
//! - The data toggles of class endpoints: the device's DPID against the host's own, mismatches listed.
//!
//! What it watches, in [Inner::errors]: a pop from an empty receive FIFO or past an entry's data, a push past a
//! transmit FIFO's depth or into an endpoint that isn't enabled (§62.15.6: "the application must first write into
//! the endpoint control register, before writing the data into the data FIFO"), a write that sets EPENA on an
//! endpoint already enabled, EPDIS on an OUT endpoint without GONAKEFF, a SETUP with STUPCNT 0 or a full receive FIFO, a data toggle mismatch.

extern crate std;

use core::cell::RefCell;
use std::collections::VecDeque;
use std::format;
use std::string::String;
use std::vec::Vec;

use super::regs::{Regs, ctl, daint, dsts, epint, gahbcfg, gint, grstctl, grxsts, offset, tsiz};
use crate::Speed;

/// Endpoints the simulated core has.
pub const ENDPOINTS: usize = 9;

/// The host's view of a transaction.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum Handshake {
    Ack,
    Nak,
    Stall,
    /// No answer: the endpoint is not active.
    None,
}

/// A receive FIFO entry: the status word GRXSTSP gives, and its data.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Entry {
    pub status: u32,
    pub data: Vec<u32>,
}

impl Entry {
    fn new(pktsts: u32, n: usize, bytes: &[u8]) -> Self {
        Entry {
            status: pktsts << grxsts::PKTSTS_SHIFT
                | (bytes.len() as u32) << grxsts::BCNT_SHIFT
                | n as u32,
            data: words(bytes),
        }
    }

    fn pktsts(&self) -> u32 {
        (self.status & grxsts::PKTSTS) >> grxsts::PKTSTS_SHIFT
    }

    fn ep(&self) -> usize {
        (self.status & grxsts::EPNUM) as usize
    }
}

fn words(bytes: &[u8]) -> Vec<u32> {
    bytes
        .chunks(4)
        .map(|c| {
            let mut w = [0u8; 4];
            w[..c.len()].copy_from_slice(c);
            u32::from_le_bytes(w)
        })
        .collect()
}

/// The core's state, open to the test.
pub struct Inner {
    /// The registers below the FIFOs, by offset / 4.
    pub regs: [u32; 0x400],
    pub rx: VecDeque<Entry>,
    /// Data words of the entry last popped, not popped yet.
    pub rx_data: VecDeque<u32>,
    pub tx: [VecDeque<u32>; ENDPOINTS],
    /// A SETUP was taken and its setup-done entry waits for the host's next token to EP0.
    pub setup_pending: bool,
    /// The host's data toggles per endpoint: the PID of its next OUT packet, and the PID it expects next IN.
    pub host_out_pid: [bool; ENDPOINTS],
    pub host_in_pid: [bool; ENDPOINTS],
    /// What went against the rules (see the module docs).
    pub errors: Vec<String>,
    /// SGONAK is set: OUT data is NAKed, the pattern is in the receive FIFO or popped.
    pub global_out_nak: bool,
    /// The Global OUT NAK pattern was popped: GONAKEFF.
    pub global_out_nak_effective: bool,
    /// The waits the driver asked for, in µs.
    pub delays_us: Vec<u32>,
}

impl Inner {
    fn reg(&self, offset: usize) -> u32 {
        self.regs[offset / 4]
    }

    fn reg_mut(&mut self, offset: usize) -> &mut u32 {
        &mut self.regs[offset / 4]
    }

    /// Words in the receive FIFO, status words included.
    pub fn rx_used(&self) -> usize {
        self.rx.iter().map(|e| 1 + e.data.len()).sum()
    }

    fn rx_depth(&self) -> usize {
        (self.reg(offset::GRXFSIZ) & 0xffff) as usize
    }

    /// Depth of transmit FIFO `n` in words.
    pub fn tx_depth(&self, n: usize) -> usize {
        (self.reg(offset::dieptxf(n)) >> 16) as usize
    }

    fn daint(&self) -> u32 {
        let im = self.reg(offset::DIEPMSK);
        let om = self.reg(offset::DOEPMSK);
        let mut v = 0;
        for n in 0..ENDPOINTS {
            if self.reg(offset::diepint(n)) & im != 0 {
                v |= daint::in_ep(n);
            }
            if self.reg(offset::doepint(n)) & om != 0 {
                v |= daint::out_ep(n);
            }
        }
        v
    }

    fn gintsts(&self) -> u32 {
        let mut v = self.reg(offset::GINTSTS)
            & !(gint::RXFLVL | gint::IEPINT | gint::OEPINT | gint::GONAKEFF);
        if self.global_out_nak_effective {
            v |= gint::GONAKEFF;
        }
        if !self.rx.is_empty() {
            v |= gint::RXFLVL;
        }
        let d = self.daint() & self.reg(offset::DAINTMSK);
        if d & 0xffff != 0 {
            v |= gint::IEPINT;
        }
        if d >> 16 != 0 {
            v |= gint::OEPINT;
        }
        v
    }

    fn error(&mut self, e: String) {
        self.errors.push(e);
    }

    /// A control register write (§62.14.54, §62.14.64).
    fn write_ctl(&mut self, c: usize, i: usize, n: usize, inp: bool, value: u32) {
        let before = self.reg(c);
        let keep = if n == 0 {
            // EP0: MPSIZ (IN only), STALL set-only, TXFNUM
            (value & (ctl::MPSIZ | ctl::TXFNUM)) | ((before | value) & ctl::STALL)
        } else {
            value & ctl::KEEP
        };
        let mut v = keep | (before & (ctl::EPENA | ctl::NAKSTS | ctl::DPID));
        if value & ctl::SNAK != 0 {
            v |= ctl::NAKSTS;
            if inp {
                *self.reg_mut(i) |= epint::INEPNE;
            }
        }
        if value & ctl::CNAK != 0 {
            v &= !ctl::NAKSTS;
        }
        if value & ctl::SD0PID != 0 {
            v &= !ctl::DPID;
        }
        if value & (1 << 29) != 0 {
            v |= ctl::DPID;
        }
        if value & ctl::EPENA != 0 {
            if before & ctl::EPENA != 0 {
                self.error(format!(
                    "EPENA written on enabled {} {n}: {value:#010x}",
                    if inp { "IN" } else { "OUT" }
                ));
            }
            v |= ctl::EPENA;
        }
        if value & ctl::EPDIS != 0 && !inp && !self.global_out_nak_effective {
            self.error(format!("EPDIS on OUT {n} without the global OUT NAK"));
        }
        if value & ctl::EPDIS != 0 && before & ctl::EPENA != 0 {
            v &= !ctl::EPENA;
            *self.reg_mut(i) |= epint::EPDISD;
        }
        *self.reg_mut(c) = v;
    }

    fn read_ctl(&self, offset: usize, n: usize) -> u32 {
        let v = self.reg(offset);
        if n == 0 { v | ctl::USBAEP } else { v }
    }

    fn core_reset(&mut self) {
        self.regs = [0; 0x400];
        self.rx.clear();
        self.rx_data.clear();
        for t in self.tx.iter_mut() {
            t.clear();
        }
        self.setup_pending = false;
        self.global_out_nak = false;
        self.global_out_nak_effective = false;
    }

    fn pop_status(&mut self) -> u32 {
        if !self.rx_data.is_empty() {
            let left = self.rx_data.len();
            self.error(format!("status popped with {left} data words left"));
            self.rx_data.clear();
        }
        let Some(e) = self.rx.pop_front() else {
            self.error(String::from("GRXSTSP read with the receive FIFO empty"));
            return 0;
        };
        let n = e.ep();
        match e.pktsts() {
            grxsts::SETUP_DONE => {
                *self.reg_mut(offset::doepctl(0)) &= !ctl::EPENA;
                *self.reg_mut(offset::doepint(0)) |= epint::STUP;
            }
            grxsts::GLOBAL_OUT_NAK => self.global_out_nak_effective = true,
            grxsts::OUT_DONE => {
                *self.reg_mut(offset::doepctl(n)) &= !ctl::EPENA;
                *self.reg_mut(offset::doepint(n)) |= epint::XFRC;
            }
            _ => {}
        }
        self.rx_data = e.data.into();
        e.status
    }

    fn push_rx(&mut self, e: Entry) -> bool {
        if self.rx_used() + 1 + e.data.len() > self.rx_depth() {
            return false;
        }
        self.rx.push_back(e);
        true
    }

    /// The setup-done entry of a SETUP taken before, at the host's next token to EP0.
    fn setup_done(&mut self) {
        if self.setup_pending {
            self.setup_pending = false;
            if !self.push_rx(Entry::new(grxsts::SETUP_DONE, 0, &[])) {
                self.error(String::from("no room for the setup-done entry"));
            }
        }
    }

    /// EP0's max packet size from DIEPCTL0.MPSIZ, a class endpoint's from its MPSIZ.
    fn max_packet(&self, n: usize, inp: bool) -> usize {
        if n == 0 {
            match self.reg(offset::diepctl(0)) & 0b11 {
                0b00 => 64,
                0b01 => 32,
                0b10 => 16,
                _ => 8,
            }
        } else {
            let c = if inp {
                offset::diepctl(n)
            } else {
                offset::doepctl(n)
            };
            (self.reg(c) & ctl::MPSIZ) as usize
        }
    }

    /// Decrements XFRSIZ and PKTCNT of `tsiz` by a packet of `len`; whether the transfer is complete (no packets
    /// left, or a short packet).
    fn count_packet(&mut self, tsiz_offset: usize, len: usize, mps: usize, n: usize) -> bool {
        let t = self.reg(tsiz_offset);
        let (xfr_mask, pkt_mask) = if n == 0 {
            (0x7f, 0b11 << tsiz::PKTCNT_SHIFT)
        } else {
            (tsiz::XFRSIZ, tsiz::PKTCNT)
        };
        let xfr = (t & xfr_mask) as usize;
        let pkt = (t & pkt_mask) >> tsiz::PKTCNT_SHIFT;
        let xfr = xfr.saturating_sub(len);
        let pkt = pkt.saturating_sub(1);
        *self.reg_mut(tsiz_offset) =
            (t & !(xfr_mask | pkt_mask)) | xfr as u32 | pkt << tsiz::PKTCNT_SHIFT;
        pkt == 0 || len < mps
    }
}

/// The simulated core. `&Sim` implements [Regs].
pub struct Sim {
    inner: RefCell<Inner>,
}

impl Default for Sim {
    fn default() -> Self {
        Self::new()
    }
}

impl Sim {
    /// The core after a system reset: every register 0.
    pub fn new() -> Self {
        Sim {
            inner: RefCell::new(Inner {
                regs: [0; 0x400],
                rx: VecDeque::new(),
                rx_data: VecDeque::new(),
                tx: Default::default(),
                setup_pending: false,
                global_out_nak: false,
                global_out_nak_effective: false,
                host_out_pid: [false; ENDPOINTS],
                host_in_pid: [false; ENDPOINTS],
                errors: Vec::new(),
                delays_us: Vec::new(),
            }),
        }
    }

    /// The state, to read or change.
    pub fn with<T>(&self, f: impl FnOnce(&mut Inner) -> T) -> T {
        f(&mut self.inner.borrow_mut())
    }

    /// A register as the driver reads it.
    pub fn reg(&self, offset: usize) -> u32 {
        (&self).read(offset)
    }

    /// Takes the rule violations so far.
    pub fn take_errors(&self) -> Vec<String> {
        core::mem::take(&mut self.inner.borrow_mut().errors)
    }

    /// Whether the interrupt line is up: GAHBCFG.GINT and a pending flag that GINTMSK unmasks.
    pub fn irq(&self) -> bool {
        let i = self.inner.borrow();
        i.reg(offset::GAHBCFG) & gahbcfg::GINT != 0 && i.gintsts() & i.reg(offset::GINTMSK) != 0
    }

    /// A USB reset: USBRST, and "The core clears this bit [USBAEP] for all endpoints (other than EP 0) after
    /// detecting a USB reset" (§62.14.54). The host's data toggles start over.
    pub fn bus_reset(&self) {
        let mut i = self.inner.borrow_mut();
        for n in 1..ENDPOINTS {
            *i.reg_mut(offset::diepctl(n)) &= !ctl::USBAEP;
            *i.reg_mut(offset::doepctl(n)) &= !ctl::USBAEP;
        }
        *i.reg_mut(offset::GINTSTS) |= gint::USBRST;
        *i.reg_mut(offset::DSTS) &= !dsts::SUSPSTS;
        i.setup_pending = false;
        i.host_out_pid = [false; ENDPOINTS];
        i.host_in_pid = [false; ENDPOINTS];
    }

    /// The end of the reset's speed detection: DSTS.ENUMSPD and ENUMDNE.
    pub fn enum_done(&self, speed: Speed) {
        let mut i = self.inner.borrow_mut();
        let spd = match speed {
            Speed::High => dsts::ENUMSPD_HIGH,
            Speed::Full => 0b01,
        };
        let d = i.reg(offset::DSTS) & !dsts::ENUMSPD;
        *i.reg_mut(offset::DSTS) = d | spd << dsts::ENUMSPD_SHIFT;
        *i.reg_mut(offset::GINTSTS) |= gint::ENUMDNE;
    }

    pub fn suspend(&self) {
        let mut i = self.inner.borrow_mut();
        *i.reg_mut(offset::DSTS) |= dsts::SUSPSTS;
        *i.reg_mut(offset::GINTSTS) |= gint::USBSUSP;
    }

    pub fn resume(&self) {
        let mut i = self.inner.borrow_mut();
        *i.reg_mut(offset::DSTS) &= !dsts::SUSPSTS;
        *i.reg_mut(offset::GINTSTS) |= gint::WKUPINT;
    }

    /// The device address in DCFG.
    pub fn address(&self) -> u8 {
        ((self.reg(offset::DCFG) >> 4) & 0x7f) as u8
    }

    /// Whether the device is connected (DCTL.SDIS clear).
    pub fn connected(&self) -> bool {
        self.reg(offset::DCTL) & super::regs::dctl::SDIS == 0
    }

    /// The host sets its data toggles of endpoint `n` to DATA0, as after SET_CONFIGURATION, SET_INTERFACE or
    /// CLEAR_FEATURE(ENDPOINT_HALT).
    pub fn host_reset_toggles(&self, n: usize) {
        let mut i = self.inner.borrow_mut();
        i.host_out_pid[n] = false;
        i.host_in_pid[n] = false;
    }

    /// A SETUP transaction to EP0 (§62.15.6 "SETUP and OUT data transfers"): the core takes it "irrespective of
    /// the NAK status and EPENA bit setting", "internally sets the IN NAK and OUT NAK bits for the control IN/OUT
    /// endpoints on which the SETUP packet was received", decrements STUPCNT. Always Ack: a device must take a
    /// SETUP; one it can't is an error.
    pub fn host_setup(&self, data: [u8; 8]) -> Handshake {
        let mut i = self.inner.borrow_mut();
        let t = i.reg(offset::doeptsiz(0));
        let stupcnt = (t & tsiz::STUPCNT) >> tsiz::STUPCNT_SHIFT;
        if stupcnt == 0 {
            i.error(String::from("SETUP with STUPCNT 0"));
        }
        *i.reg_mut(offset::doeptsiz(0)) =
            (t & !tsiz::STUPCNT) | stupcnt.saturating_sub(1) << tsiz::STUPCNT_SHIFT;
        if !i.push_rx(Entry::new(grxsts::SETUP_DATA, 0, &data)) {
            i.error(String::from("SETUP with the receive FIFO full"));
        }
        for c in [offset::diepctl(0), offset::doepctl(0)] {
            let v = i.reg(c);
            *i.reg_mut(c) = (v | ctl::NAKSTS) & !ctl::STALL;
        }
        i.setup_pending = true;
        Handshake::Ack
    }

    /// An OUT transaction with `data` to endpoint `n`.
    pub fn host_out(&self, n: usize, data: &[u8]) -> Handshake {
        let mut i = self.inner.borrow_mut();
        if n == 0 {
            i.setup_done();
        }
        let c = i.read_ctl(offset::doepctl(n), n);
        if c & ctl::USBAEP == 0 {
            return Handshake::None;
        }
        if c & ctl::STALL != 0 {
            return Handshake::Stall;
        }
        // "the core stops writing data, except SETUP packets, to the receive FIFO" (Setting the global OUT NAK)
        if c & ctl::EPENA == 0 || c & ctl::NAKSTS != 0 || i.global_out_nak {
            return Handshake::Nak;
        }
        let mps = i.max_packet(n, false);
        if data.len() > mps {
            i.error(format!("OUT {n}: {} bytes, max packet {mps}", data.len()));
        }
        let entry = Entry::new(grxsts::OUT_DATA, n, data);
        if i.rx_used() + 1 + entry.data.len() + 1 > i.rx_depth() {
            return Handshake::Nak;
        }
        if n != 0 {
            let pid = i.host_out_pid[n];
            i.host_out_pid[n] = !pid;
            if pid != (c & ctl::DPID != 0) {
                // the device ACKs and drops a packet with the wrong toggle (USB 2.0 §8.6.3)
                i.error(format!(
                    "OUT {n}: host DATA{} device DATA{}",
                    pid as u8,
                    (!pid) as u8
                ));
                return Handshake::Ack;
            }
            *i.reg_mut(offset::doepctl(n)) ^= ctl::DPID;
        }
        i.push_rx(entry);
        if i.count_packet(offset::doeptsiz(n), data.len(), mps, n) {
            i.push_rx(Entry::new(grxsts::OUT_DONE, n, &[]));
            *i.reg_mut(offset::doepctl(n)) |= ctl::NAKSTS;
        }
        Handshake::Ack
    }

    /// An IN transaction to endpoint `n`: the packet, or the handshake that came instead.
    pub fn host_in(&self, n: usize) -> Result<Vec<u8>, Handshake> {
        let mut i = self.inner.borrow_mut();
        if n == 0 {
            i.setup_done();
        }
        let c = i.read_ctl(offset::diepctl(n), n);
        if c & ctl::USBAEP == 0 {
            return Err(Handshake::None);
        }
        if c & ctl::STALL != 0 {
            return Err(Handshake::Stall);
        }
        if c & ctl::EPENA == 0 || c & ctl::NAKSTS != 0 {
            return Err(Handshake::Nak);
        }
        let mps = i.max_packet(n, true);
        let t = i.reg(offset::dieptsiz(n));
        let xfr = (t & if n == 0 { 0x7f } else { tsiz::XFRSIZ }) as usize;
        let len = xfr.min(mps);
        let w = len.div_ceil(4);
        if i.tx[n].len() < w {
            return Err(Handshake::Nak);
        }
        let mut data = Vec::with_capacity(len);
        for _ in 0..w {
            let word = i.tx[n].pop_front().unwrap();
            data.extend_from_slice(&word.to_le_bytes());
        }
        data.truncate(len);
        if n != 0 {
            let pid = c & ctl::DPID != 0;
            if pid != i.host_in_pid[n] {
                let host = i.host_in_pid[n] as u8;
                i.error(format!(
                    "IN {n}: device DATA{} host expects DATA{host}",
                    pid as u8
                ));
            }
            i.host_in_pid[n] = !pid;
            *i.reg_mut(offset::diepctl(n)) ^= ctl::DPID;
        }
        if i.count_packet(offset::dieptsiz(n), len, mps, n) {
            *i.reg_mut(offset::diepctl(n)) &= !ctl::EPENA;
            *i.reg_mut(offset::diepint(n)) |= epint::XFRC;
        }
        Ok(data)
    }
}

impl Regs for &Sim {
    fn read(&self, offset: usize) -> u32 {
        if offset >= offset::fifo(0) {
            return self.pop();
        }
        let mut i = self.inner.borrow_mut();
        match offset {
            offset::GINTSTS => i.gintsts(),
            offset::GRXSTSP => i.pop_status(),
            offset::GRSTCTL => i.reg(offset) | grstctl::AHBIDL,
            offset::DAINT => i.daint(),
            o if (0x900..0xb00).contains(&o) && o % 0x20 == 0 => i.read_ctl(o, (o - 0x900) / 0x20),
            o if (0xb00..0xd00).contains(&o) && o % 0x20 == 0 => i.read_ctl(o, (o - 0xb00) / 0x20),
            o if (0x918..0xb00).contains(&o) && o % 0x20 == 0x18 => {
                let n = (o - 0x918) / 0x20;
                (i.tx_depth(n) - i.tx[n].len().min(i.tx_depth(n))) as u32
            }
            o => i.reg(o),
        }
    }

    fn write(&self, offset: usize, value: u32) {
        if offset >= offset::fifo(0) {
            return self.push(offset / 0x1000 - 1, value);
        }
        let mut i = self.inner.borrow_mut();
        match offset {
            offset::GINTSTS => *i.reg_mut(offset) &= !value,
            offset::GRSTCTL => {
                if value & grstctl::CSRST != 0 {
                    i.core_reset();
                }
                if value & grstctl::RXFFLSH != 0 {
                    i.rx.clear();
                    i.rx_data.clear();
                }
                if value & grstctl::TXFFLSH != 0 {
                    let f = (value >> grstctl::TXFNUM_SHIFT) & 0x1f;
                    for (n, t) in i.tx.iter_mut().enumerate() {
                        if f == grstctl::TXFNUM_ALL || f as usize == n {
                            t.clear();
                        }
                    }
                }
                *i.reg_mut(offset) =
                    value & !(grstctl::CSRST | grstctl::RXFFLSH | grstctl::TXFFLSH);
            }
            offset::DCTL => {
                use super::regs::dctl::{CGONAK, SGONAK};
                if value & SGONAK != 0 && !i.global_out_nak {
                    i.global_out_nak = true;
                    if !i.push_rx(Entry::new(grxsts::GLOBAL_OUT_NAK, 0, &[])) {
                        i.error(String::from("no room for the Global OUT NAK pattern"));
                    }
                }
                if value & CGONAK != 0 {
                    i.global_out_nak = false;
                    i.global_out_nak_effective = false;
                }
                *i.reg_mut(offset) = value & !(SGONAK | CGONAK);
            }
            offset::DAINT | offset::GRXSTSP | offset::DSTS => {}
            o if (0x900..0xb00).contains(&o) && o % 0x20 == 0 => {
                let n = (o - 0x900) / 0x20;
                i.write_ctl(o, offset::diepint(n), n, true, value)
            }
            o if (0xb00..0xd00).contains(&o) && o % 0x20 == 0 => {
                let n = (o - 0xb00) / 0x20;
                i.write_ctl(o, offset::doepint(n), n, false, value)
            }
            o if (0x908..0xd00).contains(&o) && o % 0x20 == 0x08 => *i.reg_mut(o) &= !value,
            o => *i.reg_mut(o) = value,
        }
    }

    fn pop(&self) -> u32 {
        let mut i = self.inner.borrow_mut();
        match i.rx_data.pop_front() {
            Some(w) => w,
            None => {
                i.error(String::from("data popped past the entry"));
                0
            }
        }
    }

    fn push(&self, n: usize, word: u32) {
        let mut i = self.inner.borrow_mut();
        if i.read_ctl(offset::diepctl(n), n) & ctl::EPENA == 0 {
            i.error(format!("push into IN {n} while it is not enabled"));
        }
        if i.tx[n].len() >= i.tx_depth(n) {
            i.error(format!("push past the depth of transmit FIFO {n}"));
            return;
        }
        i.tx[n].push_back(word);
    }

    fn delay_us(&self, us: u32) {
        self.inner.borrow_mut().delays_us.push(us);
    }
}
