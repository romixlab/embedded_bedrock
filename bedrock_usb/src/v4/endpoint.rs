//! Typed class endpoints: [BulkOut], [BulkIn] (single- or double-buffered) and [InterruptIn], each compiling only
//! the path of its buffering. Made by [super::Driver]'s `bulk_out` / `bulk_in` / `interrupt_in`, which check them
//! against the descriptors at compile time.
//!
//! Endpoint state is read from the hardware: a single-buffered OUT endpoint has a packet when STATRX reads NAK,
//! a single-buffered IN endpoint is free when STATTX reads NAK; a double-buffered one goes by its queue, which the
//! interrupt handler moves on each transfer.

use core::future::poll_fn;
use core::marker::PhantomData;
use core::task::Poll;

use super::chep::{self, Stat};
use super::driver::{EndpointError, OUT_PACKET_DELAY_NS, bd_count, release};
use super::regs::{Regs, bd, chep as bits};
use super::state::{EpState, Queue};
use crate::Direction;
use crate::buffer::{Double, Mode, Single};
use crate::layout::{Reg, RegMode};

/// Copies `data` into USB SRAM at byte `offset` (word aligned), whole words only (R11, RM0444 Table 225: "access
/// scheme 32 bits"). Fixed-size conversions per word: a copy with a runtime length per word calls memcpy, which
/// dominated embassy-stm32's per-packet CPU time.
pub(crate) fn write_packet<R: Regs>(r: &R, offset: u16, data: &[u8]) {
    let base = offset as usize / 4;
    let (words, tail) = data.as_chunks::<4>();
    for (i, w) in words.iter().enumerate() {
        r.set_sram(base + i, u32::from_le_bytes(*w));
    }
    if !tail.is_empty() {
        let mut w = [0u8; 4];
        w[..tail.len()].copy_from_slice(tail);
        r.set_sram(base + words.len(), u32::from_le_bytes(w));
    }
}

/// Copies `buf.len()` bytes out of USB SRAM at byte `offset`, whole words only (R11).
pub(crate) fn read_packet<R: Regs>(r: &R, offset: u16, buf: &mut [u8]) {
    let base = offset as usize / 4;
    let (words, tail) = buf.as_chunks_mut::<4>();
    let n = words.len();
    for (i, w) in words.iter_mut().enumerate() {
        *w = r.sram(base + i).to_le_bytes();
    }
    if !tail.is_empty() {
        let len = tail.len();
        tail.copy_from_slice(&r.sram(base + n).to_le_bytes()[..len]);
    }
}

/// Sets the count of the transmit buffer in descriptor word `word` of register `n`, keeping its address.
fn set_tx_count<R: Regs>(r: &R, n: usize, word: usize, offset: u16, len: usize) {
    r.set_sram(
        2 * n + word,
        offset as u32 | ((len as u32) << bd::COUNT_SHIFT),
    );
}

/// Waits until the single-buffered `dir` of register `n` is ready: STAT NAK (a packet in, or the buffer free);
/// `Err` when disabled. STALL waits: the host clears it.
async fn wait_nak<R: Regs>(
    r: &R,
    n: usize,
    dir: Direction,
    ep: &EpState,
) -> Result<(), EndpointError> {
    poll_fn(|cx| {
        match dir {
            Direction::Out => ep.out_waker.register(cx.waker()),
            Direction::In => ep.in_waker.register(cx.waker()),
        }
        let v = r.chep(n);
        let stat = match dir {
            Direction::Out => chep::stat_rx(v),
            Direction::In => chep::stat_tx(v),
        };
        match stat {
            Stat::Disabled => Poll::Ready(Err(EndpointError::Disabled)),
            Stat::Nak => Poll::Ready(Ok(())),
            Stat::Stall | Stat::Valid => Poll::Pending,
        }
    })
    .await
}

/// Waits until a class endpoint is enabled (SET_CONFIGURATION).
async fn wait_enabled<R: Regs>(r: &R, n: usize, dir: Direction, ep: &EpState) {
    poll_fn(|cx| {
        let stat = match dir {
            Direction::Out => {
                ep.out_waker.register(cx.waker());
                chep::stat_rx(r.chep(n))
            }
            Direction::In => {
                ep.in_waker.register(cx.waker());
                chep::stat_tx(r.chep(n))
            }
        };
        if stat == Stat::Disabled {
            Poll::Pending
        } else {
            Poll::Ready(())
        }
    })
    .await
}

/// A bulk OUT endpoint, number `EP`, buffering `B`.
pub struct BulkOut<R: Regs + Copy, const EP: u8, B: Mode> {
    regs: R,
    ep: &'static EpState,
    /// Byte offsets of the receive buffers (single: word 1 only).
    buffer: [u16; 2],
    max_packet: u16,
    _b: PhantomData<B>,
}

/// A bulk IN endpoint, number `EP`, buffering `B`.
pub struct BulkIn<R: Regs + Copy, const EP: u8, B: Mode> {
    regs: R,
    ep: &'static EpState,
    /// Byte offsets of the transmit buffers (single: word 0 only).
    buffer: [u16; 2],
    max_packet: u16,
    _b: PhantomData<B>,
}

/// An interrupt IN endpoint, number `EP`, single-buffered.
pub struct InterruptIn<R: Regs + Copy, const EP: u8> {
    inner: BulkIn<R, EP, Single>,
}

fn size(reg: &Reg, dir: Direction) -> u16 {
    match (reg.mode, dir) {
        (RegMode::Single { out_size, .. }, Direction::Out) => out_size,
        (RegMode::Single { in_size, .. }, Direction::In) => in_size,
        (RegMode::Double { size, .. }, _) => size,
        (RegMode::Unused, _) => 0,
    }
}

impl<R: Regs + Copy, const EP: u8, B: Mode> BulkOut<R, EP, B> {
    pub(crate) fn new(regs: R, ep: &'static EpState, reg: &Reg) -> Self {
        BulkOut {
            regs,
            ep,
            buffer: reg.buffer,
            max_packet: size(reg, Direction::Out),
            _b: PhantomData,
        }
    }

    pub const fn max_packet(&self) -> u16 {
        self.max_packet
    }

    /// Waits until the endpoint is enabled.
    pub async fn wait_enabled(&mut self) {
        wait_enabled(&self.regs, EP as usize, Direction::Out, self.ep).await
    }
}

impl<R: Regs + Copy, const EP: u8> BulkOut<R, EP, Single> {
    /// Receives one packet into `buf` and gives its length.
    ///
    /// R3, RM0444 §37.5.2: "clear the interrupt flag bit and get the number of received bytes reading the
    /// COUNTn_RX location ... After the received data is processed, the application software must set the STATRX
    /// bits to 11 (VALID)"; and the ES0548 2.16.1 wait between seeing the packet and reading it (R6). A packet
    /// longer than `buf` stays where it is and gives [EndpointError::BufferOverflow].
    pub async fn read(&mut self, buf: &mut [u8]) -> Result<usize, EndpointError> {
        let n = EP as usize;
        let r = &self.regs;
        wait_nak(r, n, Direction::Out, self.ep).await?;
        critical_section::with(|_| {
            let v = r.chep(n);
            if v & bits::VTRX != 0 {
                r.set_chep(n, chep::clear_vtrx(v));
            }
            r.delay_ns(OUT_PACKET_DELAY_NS);
            let len = bd_count(r.sram(2 * n + 1));
            if len > buf.len() {
                return Err(EndpointError::BufferOverflow);
            }
            read_packet(r, self.buffer[1], &mut buf[..len]);
            let v = r.chep(n);
            r.set_chep(n, chep::to_stat_rx(chep::invariant(v), v, Stat::Valid));
            Ok(len)
        })
    }
}

impl<R: Regs + Copy, const EP: u8> BulkOut<R, EP, Double> {
    /// Receives one packet into `buf` and gives its length; the peripheral fills the other buffer meanwhile.
    /// The R6 wait comes before the count is read. A packet longer than `buf` stays queued and gives
    /// [EndpointError::BufferOverflow].
    pub async fn read(&mut self, buf: &mut [u8]) -> Result<usize, EndpointError> {
        let n = EP as usize;
        let r = &self.regs;
        let ep = self.ep;
        let buffer = self.buffer;
        poll_fn(|cx| {
            ep.out_waker.register(cx.waker());
            critical_section::with(|cs| {
                if chep::stat_rx(r.chep(n)) == Stat::Disabled {
                    return Poll::Ready(Err(EndpointError::Disabled));
                }
                let Queue::Out(mut q) = ep.queue(cs) else {
                    return Poll::Ready(Err(EndpointError::Disabled));
                };
                let Some(b) = q.next() else {
                    return Poll::Pending;
                };
                r.delay_ns(OUT_PACKET_DELAY_NS);
                let word = r.sram(2 * n + b);
                let len = bd_count(word);
                if len > buf.len() {
                    return Poll::Ready(Err(EndpointError::BufferOverflow));
                }
                read_packet(r, buffer[b], &mut buf[..len]);
                // the count back to 0, the allocation kept, before the buffer is handed over again
                r.set_sram(2 * n + b, word & !bd::COUNT);
                if q.consumed() {
                    release(r, n, Direction::Out);
                }
                ep.set_queue(cs, Queue::Out(q));
                Poll::Ready(Ok(len))
            })
        })
        .await
    }
}

impl<R: Regs + Copy, const EP: u8, B: Mode> BulkIn<R, EP, B> {
    pub(crate) fn new(regs: R, ep: &'static EpState, reg: &Reg) -> Self {
        BulkIn {
            regs,
            ep,
            buffer: reg.buffer,
            max_packet: size(reg, Direction::In),
            _b: PhantomData,
        }
    }

    pub const fn max_packet(&self) -> u16 {
        self.max_packet
    }

    /// Waits until the endpoint is enabled.
    pub async fn wait_enabled(&mut self) {
        wait_enabled(&self.regs, EP as usize, Direction::In, self.ep).await
    }
}

impl<R: Regs + Copy, const EP: u8> BulkIn<R, EP, Single> {
    /// Sends one packet (up to the max packet size; shorter, or empty, ends a transfer). Returns once the packet
    /// is handed to the peripheral, not when the host has it.
    ///
    /// R3, RM0444 §37.5.2: the software clears the interrupt bit, "then prepares another buffer full of data to
    /// be sent, updates the COUNTn_TX table location ..., and finally sets STATTX to 11 (VALID) ... It is
    /// mandatory to execute the sequence of operations in the above mentioned order to avoid losing the
    /// notification of a second IN transaction".
    pub async fn write(&mut self, data: &[u8]) -> Result<(), EndpointError> {
        if data.len() > self.max_packet as usize {
            return Err(EndpointError::BufferOverflow);
        }
        let n = EP as usize;
        let r = &self.regs;
        wait_nak(r, n, Direction::In, self.ep).await?;
        critical_section::with(|_| {
            let v = r.chep(n);
            if v & bits::VTTX != 0 {
                r.set_chep(n, chep::clear_vttx(v));
            }
            write_packet(r, self.buffer[0], data);
            set_tx_count(r, n, 0, self.buffer[0], data.len());
            let v = r.chep(n);
            r.set_chep(n, chep::to_stat_tx(chep::invariant(v), v, Stat::Valid));
        });
        Ok(())
    }
}

impl<R: Regs + Copy, const EP: u8> BulkIn<R, EP, Double> {
    /// Sends one packet into the free buffer; waits only when both are queued. The interrupt handler hands the
    /// second packet to the peripheral when the first is sent.
    pub async fn write(&mut self, data: &[u8]) -> Result<(), EndpointError> {
        if data.len() > self.max_packet as usize {
            return Err(EndpointError::BufferOverflow);
        }
        let n = EP as usize;
        let r = &self.regs;
        let ep = self.ep;
        let buffer = self.buffer;
        poll_fn(|cx| {
            ep.in_waker.register(cx.waker());
            critical_section::with(|cs| {
                match chep::stat_tx(r.chep(n)) {
                    Stat::Disabled => return Poll::Ready(Err(EndpointError::Disabled)),
                    Stat::Stall => return Poll::Pending,
                    _ => {}
                }
                let Queue::In(mut q) = ep.queue(cs) else {
                    return Poll::Ready(Err(EndpointError::Disabled));
                };
                let Some(b) = q.free() else {
                    return Poll::Pending;
                };
                // software-owned buffer: the peripheral may finish the other one meanwhile, never touches this one
                write_packet(r, buffer[b], data);
                set_tx_count(r, n, b, buffer[b], data.len());
                if q.push() {
                    release(r, n, Direction::In);
                }
                ep.set_queue(cs, Queue::In(q));
                Poll::Ready(Ok(()))
            })
        })
        .await
    }
}

impl<R: Regs + Copy, const EP: u8> InterruptIn<R, EP> {
    pub(crate) fn new(regs: R, ep: &'static EpState, reg: &Reg) -> Self {
        InterruptIn {
            inner: BulkIn::new(regs, ep, reg),
        }
    }

    pub const fn max_packet(&self) -> u16 {
        self.inner.max_packet
    }

    /// Sends one report, polled by the host at the endpoint's interval.
    pub async fn write(&mut self, data: &[u8]) -> Result<(), EndpointError> {
        self.inner.write(data).await
    }

    pub async fn wait_enabled(&mut self) {
        self.inner.wait_enabled().await
    }
}
