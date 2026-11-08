//! Typed class endpoints on the OTG core: [BulkOut], [BulkIn] (single- or double-buffered) and [InterruptIn].
//! Made by [super::Driver]'s `bulk_out` / `bulk_in` / `interrupt_in`, which check them against the layout at
//! compile time.
//!
//! OUT: the interrupt handler copies each packet into the endpoint's buffer in [super::State]; `Double` has two,
//! so the core takes the next packet while the firmware reads the last. IN: one packet per transfer, written
//! into the endpoint's transmit FIFO once the transfer before it is done (EPENA clear); `write` returns when the
//! packet is in the FIFO, so the firmware prepares the next one while the core sends. `Double` sizes the FIFO for
//! two packets but still sends one per transfer.
//!
//! Whether an endpoint is enabled is USBAEP in its control register: SET_CONFIGURATION sets it, a bus reset
//! clears it ("The core clears this bit for all endpoints (other than EP 0) after detecting a USB reset",
//! RM0468 Rev 3 §62.14.54).

use core::future::poll_fn;
use core::marker::PhantomData;
use core::task::Poll;

use super::driver::{EndpointError, arm_out, set_ctl};
use super::ep0::{read_words, write_fifo};
use super::layout::{Dir, Layout};
use super::regs::{Regs, ctl, offset, tsiz};
use super::state::StateRef;
use crate::buffer::Mode;

fn active<R: Regs>(r: &R, offset: usize) -> bool {
    r.read(offset) & ctl::USBAEP != 0
}

/// A bulk OUT endpoint, number `EP`, buffering `B`.
pub struct BulkOut<R: Regs + Copy, const EP: u8, B: Mode> {
    regs: R,
    st: StateRef,
    layout: &'static Layout,
    _b: PhantomData<B>,
}

impl<R: Regs + Copy, const EP: u8, B: Mode> BulkOut<R, EP, B> {
    pub(crate) fn new(regs: R, st: StateRef, layout: &'static Layout) -> Self {
        BulkOut {
            regs,
            st,
            layout,
            _b: PhantomData,
        }
    }

    fn dir(&self) -> &'static Dir {
        &self.layout.eps[EP as usize].out
    }

    /// The max packet size at the current speed.
    pub fn max_packet(&self) -> u16 {
        self.dir().max_packet_at(self.st.speed())
    }

    /// Waits until the endpoint is enabled (SET_CONFIGURATION).
    pub async fn wait_enabled(&mut self) {
        let n = EP as usize;
        poll_fn(|cx| {
            self.st.eps[n].out_waker.register(cx.waker());
            if active(&self.regs, offset::doepctl(n)) {
                Poll::Ready(())
            } else {
                Poll::Pending
            }
        })
        .await
    }

    /// Waits for the next packet and copies it into `buf`; its length, or `Disabled` (not configured, or a bus
    /// reset), or `BufferOverflow` when `buf` is shorter than the packet (the packet is dropped).
    pub async fn read(&mut self, buf: &mut [u8]) -> Result<usize, EndpointError> {
        let n = EP as usize;
        let ep = &self.st.eps[n];
        let (b, len) = poll_fn(|cx| {
            ep.out_waker.register(cx.waker());
            critical_section::with(|cs| {
                let q = ep.out(cs);
                if !q.is_enabled() {
                    return Poll::Ready(Err(EndpointError::Disabled));
                }
                match q.next() {
                    Some(p) => Poll::Ready(Ok(p)),
                    None => Poll::Pending,
                }
            })
        })
        .await?;
        let d = self.dir();
        let len = len as usize;
        let fits = len <= buf.len();
        if fits {
            read_words(
                self.st,
                d.start as usize + b * d.words as usize,
                &mut buf[..len],
            );
        }
        critical_section::with(|cs| {
            let mut q = ep.out(cs);
            // a bus reset or deconfiguration in between started the queue over: nothing to give back
            if q.next() == Some((b, len as u16)) {
                q.consumed();
                ep.set_out(cs, q);
                arm_out(&self.regs, self.layout, self.st, n, cs);
            }
        });
        if fits {
            Ok(len)
        } else {
            Err(EndpointError::BufferOverflow)
        }
    }
}

/// Sends one packet on IN endpoint `n`: waits until its last transfer is done and it is enabled, then XFRSIZ and
/// PKTCNT for one packet, EPENA and CNAK, and the data into its FIFO (§62.15.6 "Generic non-periodic IN data
/// transfers", application programming sequence 1-3; "the application must first write into the endpoint control
/// register, before writing the data into the data FIFO"). A STALLed endpoint waits until the host clears it.
async fn send<R: Regs>(
    r: &R,
    st: StateRef,
    d: &Dir,
    n: usize,
    data: &[u8],
) -> Result<(), EndpointError> {
    if data.len() > d.max_packet_at(st.speed()) as usize {
        return Err(EndpointError::BufferOverflow);
    }
    let c = offset::diepctl(n);
    poll_fn(|cx| {
        st.eps[n].in_waker.register(cx.waker());
        let v = r.read(c);
        if v & ctl::USBAEP == 0 {
            return Poll::Ready(Err(EndpointError::Disabled));
        }
        if v & (ctl::EPENA | ctl::STALL) != 0 {
            return Poll::Pending;
        }
        critical_section::with(|_| {
            // checked again inside: a bus reset or a halt from the run task can't land in between
            let v = r.read(c);
            if v & ctl::USBAEP == 0 {
                return Poll::Ready(Err(EndpointError::Disabled));
            }
            if v & (ctl::EPENA | ctl::STALL) != 0 {
                return Poll::Pending;
            }
            r.write(offset::dieptsiz(n), tsiz::one(data.len()));
            set_ctl(r, c, |v| v, ctl::EPENA | ctl::CNAK);
            write_fifo(r, n, data);
            Poll::Ready(Ok(()))
        })
    })
    .await
}

async fn wait_in_enabled<R: Regs>(r: &R, st: StateRef, n: usize) {
    poll_fn(|cx| {
        st.eps[n].in_waker.register(cx.waker());
        if active(r, offset::diepctl(n)) {
            Poll::Ready(())
        } else {
            Poll::Pending
        }
    })
    .await
}

/// A bulk IN endpoint, number `EP`, buffering `B`.
pub struct BulkIn<R: Regs + Copy, const EP: u8, B: Mode> {
    regs: R,
    st: StateRef,
    layout: &'static Layout,
    _b: PhantomData<B>,
}

impl<R: Regs + Copy, const EP: u8, B: Mode> BulkIn<R, EP, B> {
    pub(crate) fn new(regs: R, st: StateRef, layout: &'static Layout) -> Self {
        BulkIn {
            regs,
            st,
            layout,
            _b: PhantomData,
        }
    }

    /// The max packet size at the current speed: 512 at high speed.
    pub fn max_packet(&self) -> u16 {
        self.layout.eps[EP as usize]
            .inp
            .max_packet_at(self.st.speed())
    }

    pub async fn wait_enabled(&mut self) {
        wait_in_enabled(&self.regs, self.st, EP as usize).await
    }

    /// Queues one packet, at most [BulkIn::max_packet] bytes (an empty slice is a zero-length packet); returns once
    /// it is in the FIFO.
    pub async fn write(&mut self, data: &[u8]) -> Result<(), EndpointError> {
        let n = EP as usize;
        send(&self.regs, self.st, &self.layout.eps[n].inp, n, data).await
    }
}

/// An interrupt IN endpoint, number `EP`.
pub struct InterruptIn<R: Regs + Copy, const EP: u8> {
    regs: R,
    st: StateRef,
    layout: &'static Layout,
}

impl<R: Regs + Copy, const EP: u8> InterruptIn<R, EP> {
    pub(crate) fn new(regs: R, st: StateRef, layout: &'static Layout) -> Self {
        InterruptIn { regs, st, layout }
    }

    /// Queues one report for the host's next poll; returns once it is in the FIFO.
    pub async fn write(&mut self, data: &[u8]) -> Result<(), EndpointError> {
        let n = EP as usize;
        send(&self.regs, self.st, &self.layout.eps[n].inp, n, data).await
    }

    pub async fn wait_enabled(&mut self) {
        wait_in_enabled(&self.regs, self.st, EP as usize).await
    }
}
