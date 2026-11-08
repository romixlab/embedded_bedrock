//! The control endpoint's register side on the OTG core: what the EP0 state machine (`control`) asks for, as
//! register writes, and the waits for their completion.
//!
//! RM0468 Rev 3 §62.15.6 "SETUP transactions": the core takes a SETUP "irrespective of the NAK status and EPENA
//! bit setting" while STUPCNT is not 0, "internally sets the IN NAK and OUT NAK bits for the control IN/OUT
//! endpoints on which the SETUP packet was received", and raises STUP once the host has moved on to the data or
//! status stage ("When the application pops the setup stage done word from the receive FIFO, the core interrupts
//! the application with an STUP interrupt"). The interrupt handler keeps the SETUP's two words and raises
//! [event::SETUP] on STUP. Each data or status packet is one transfer: XFRSIZ and PKTCNT for one packet, then EPENA
//! and CNAK; its XFRC is the completion. The data toggles of EP0 are the core's.

use core::future::poll_fn;
use core::task::Poll;

use super::driver::{DONE, set_ctl, stop_in};
use super::layout::Layout;
use super::regs::{Regs, ctl, offset, tsiz};
use super::state::StateRef;
use crate::v4::state::event;
use crate::{Ep0Error, Setup};

/// Endpoint 0.
pub struct Ep0<R: Regs + Copy> {
    regs: R,
    st: StateRef,
    /// Word offset of EP0's OUT buffer and its max packet size.
    out_start: usize,
    mps: usize,
}

impl<R: Regs + Copy> Ep0<R> {
    pub(crate) fn new(regs: R, st: StateRef, layout: &'static Layout) -> Self {
        Ep0 {
            regs,
            st,
            out_start: layout.eps[0].out.start as usize,
            mps: layout.eps[0].out.max_packet[0] as usize,
        }
    }

    /// Waits for `done`; a SETUP or a bus reset ends the wait first.
    async fn wait<T>(
        &self,
        resets: u8,
        done: impl Fn(critical_section::CriticalSection) -> Option<T>,
    ) -> Result<T, Ep0Error> {
        poll_fn(|cx| {
            self.st.bus.waker.register(cx.waker());
            self.st.eps[0].in_waker.register(cx.waker());
            self.st.eps[0].out_waker.register(cx.waker());
            if self.st.bus.resets() != resets {
                return Poll::Ready(Err(Ep0Error::Reset));
            }
            if self.st.bus.peek() & event::SETUP != 0 {
                return Poll::Ready(Err(Ep0Error::Setup));
            }
            match critical_section::with(&done) {
                Some(v) => Poll::Ready(Ok(v)),
                None => Poll::Pending,
            }
        })
        .await
    }

    /// Arms EP0 IN for one packet of `data` (§62.15.6 "Generic non-periodic IN data transfers": "1. Program the
    /// OTG_DIEPTSIZx register with the transfer size and corresponding packet count. 2. Program the OTG_DIEPCTLx
    /// register with the endpoint characteristics and set the CNAK and EPENA (endpoint enable) bits", then the
    /// data into the FIFO; "To write zero length packet, the application must not look at the FIFO space").
    fn send(&self, data: &[u8]) {
        let r = &self.regs;
        critical_section::with(|cs| {
            self.st.ep0.in_done.borrow(cs).set(false);
            r.write(offset::dieptsiz(0), tsiz::one(data.len()));
            set_ctl(r, offset::diepctl(0), |v| v, ctl::EPENA | ctl::CNAK);
            write_fifo(r, 0, data);
        });
    }

    /// Arms EP0 OUT for one packet, STUPCNT kept at 3.
    fn receive(&self) {
        let r = &self.regs;
        critical_section::with(|cs| {
            self.st.ep0.out_done.borrow(cs).set(None);
            r.write(
                offset::doeptsiz(0),
                3 << tsiz::STUPCNT_SHIFT | tsiz::one(self.mps),
            );
            set_ctl(r, offset::doepctl(0), |v| v, ctl::EPENA | ctl::CNAK);
        });
    }

    /// Waits for the next SETUP packet. STUPCNT goes back to 3 for the next one (§62.15.6 "Application
    /// programming sequence: 1. Program the OTG_DOEPTSIZx register. – STUPCNT = 3"); a data packet still queued on
    /// EP0 IN from the transfer the SETUP ended is stopped and flushed, or its EPENA would keep the next one out.
    pub async fn setup(&mut self) -> Setup {
        poll_fn(|cx| {
            self.st.bus.waker.register(cx.waker());
            if self.st.bus.take(event::SETUP) != 0 {
                Poll::Ready(())
            } else {
                Poll::Pending
            }
        })
        .await;
        let r = &self.regs;
        let w = critical_section::with(|cs| {
            r.write(offset::doeptsiz(0), 3 << tsiz::STUPCNT_SHIFT);
            self.st.ep0.setup.borrow(cs).get()
        });
        if r.read(offset::diepctl(0)) & ctl::EPENA != 0 {
            critical_section::with(|_| stop_in(r, 0, false));
        }
        let mut raw = [0u8; 8];
        raw[..4].copy_from_slice(&w[0].to_le_bytes());
        raw[4..].copy_from_slice(&w[1].to_le_bytes());
        Setup::parse(&raw)
    }

    /// Sends one packet of a control IN data stage and waits until the host has it.
    pub async fn data_in(&mut self, data: &[u8], _last: bool) -> Result<(), Ep0Error> {
        let resets = self.st.bus.resets();
        self.send(data);
        let st = self.st;
        self.wait(resets, |cs| st.ep0.in_done.borrow(cs).get().then_some(()))
            .await
    }

    /// The zero-length OUT status stage of a control read; waits until it is in.
    pub async fn status_out(&mut self) -> Result<(), Ep0Error> {
        self.data_out(&mut [], true).await.map(|_| ())
    }

    /// Receives one packet of a control OUT data stage into `buf` and gives its length.
    pub async fn data_out(&mut self, buf: &mut [u8], _last: bool) -> Result<usize, Ep0Error> {
        let resets = self.st.bus.resets();
        self.receive();
        let st = self.st;
        let len = self
            .wait(resets, |cs| match st.ep0.out_done.borrow(cs).get() {
                Some(v) if v & DONE != 0 => Some((v & !DONE) as usize),
                _ => None,
            })
            .await?;
        if len > buf.len() {
            return Err(Ep0Error::BufferOverflow);
        }
        read_words(self.st, self.out_start, &mut buf[..len]);
        Ok(len)
    }

    /// The zero-length IN status stage; waits until the host has it. SET_ADDRESS's address is in DCFG already.
    pub async fn status_in(&mut self) -> Result<(), Ep0Error> {
        self.data_in(&[], true).await
    }

    /// Both directions STALL: "The application can only set this bit, and the core clears it, when a SETUP token
    /// is received for this endpoint" (§62.14.60, STALL).
    pub fn stall(&mut self) {
        let r = &self.regs;
        critical_section::with(|_| {
            set_ctl(r, offset::diepctl(0), |v| v | ctl::STALL, 0);
            set_ctl(r, offset::doepctl(0), |v| v | ctl::STALL, 0);
        });
    }

    /// Between transfers: nothing to do, the core takes the next SETUP on its own.
    pub fn idle(&mut self) {}
}

/// Pushes `data` into transmit FIFO `n` in whole words, the last one padded.
pub(crate) fn write_fifo<R: Regs>(r: &R, n: usize, data: &[u8]) {
    let (words, tail) = data.as_chunks::<4>();
    for w in words {
        r.push(n, u32::from_le_bytes(*w));
    }
    if !tail.is_empty() {
        let mut w = [0u8; 4];
        w[..tail.len()].copy_from_slice(tail);
        r.push(n, u32::from_le_bytes(w));
    }
}

/// Copies `buf.len()` bytes out of the OUT buffers from word `start`.
pub(crate) fn read_words(st: StateRef, start: usize, buf: &mut [u8]) {
    let (words, tail) = buf.as_chunks_mut::<4>();
    let n = words.len();
    for (i, w) in words.iter_mut().enumerate() {
        // Safety: the caller's endpoint has this buffer ready, the interrupt handler doesn't write it
        *w = unsafe { st.word(start + i) }.to_le_bytes();
    }
    if !tail.is_empty() {
        let len = tail.len();
        tail.copy_from_slice(&unsafe { st.word(start + n) }.to_le_bytes()[..len]);
    }
}

impl<R: Regs + Copy> crate::ControlEndpoint for Ep0<R> {
    async fn setup(&mut self) -> Setup {
        Ep0::setup(self).await
    }
    async fn data_in(&mut self, data: &[u8], last: bool) -> Result<(), Ep0Error> {
        Ep0::data_in(self, data, last).await
    }
    async fn status_out(&mut self) -> Result<(), Ep0Error> {
        Ep0::status_out(self).await
    }
    async fn data_out(&mut self, buf: &mut [u8], last: bool) -> Result<usize, Ep0Error> {
        Ep0::data_out(self, buf, last).await
    }
    async fn status_in(&mut self) -> Result<(), Ep0Error> {
        Ep0::status_in(self).await
    }
    fn stall(&mut self) {
        Ep0::stall(self)
    }
    fn idle(&mut self) {
        Ep0::idle(self)
    }
}
