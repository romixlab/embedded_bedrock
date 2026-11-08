//! The control endpoint's register side: what the EP0 state machine (`control`) asks for, as register writes,
//! and the waits for their completion.
//!
//! R5, RM0444 §37.5.2: a SETUP leaves both STAT at NAK; "At all data stages before the last, the unused
//! direction must be set to STALL, so that, if the host reverses the transfer direction too soon, it gets a STALL
//! as a status stage. While enabling the last data stage, the opposite direction must be set to NAK"; "If the
//! status stage is an OUT, the STATUS_OUT (EPKIND ...) bit must be set". The data toggles of EP0 are the
//! hardware's: a SETUP sets them, each packet toggles them, nothing here writes them.

use core::future::poll_fn;
use core::task::Poll;

use super::chep::{self, Stat};
use super::driver::{OUT_PACKET_DELAY_NS, bd_count};
use super::endpoint::{read_packet, write_packet};
use super::regs::{Regs, bd, chep as bits};
use super::state::{BusState, EpState, event};
use crate::Setup;
use crate::layout::RegType;

/// Why an EP0 transfer ended early.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum Ep0Error {
    /// A new SETUP arrived: the transfer is over, [Ep0::setup] gives the new one.
    Setup,
    /// A bus reset.
    Reset,
    /// An OUT data packet longer than the buffer given.
    BufferOverflow,
}

/// Endpoint 0.
pub struct Ep0<R: Regs + Copy> {
    regs: R,
    bus: &'static BusState,
    ep: &'static EpState,
    /// Byte offsets of the transmit (word 0) and receive (word 1) buffers.
    buffer: [u16; 2],
}

const CONTROL: u32 = RegType::Control.utype();

impl<R: Regs + Copy> Ep0<R> {
    pub(crate) fn new(
        regs: R,
        bus: &'static BusState,
        ep: &'static EpState,
        buffer: [u16; 2],
    ) -> Self {
        Ep0 {
            regs,
            bus,
            ep,
            buffer,
        }
    }

    /// Sets both directions of EP0 in one write: STATUS_OUT as given, the toggles from the value just read.
    fn arm(&self, rx: Stat, tx: Stat, status_out: bool) {
        let r = &self.regs;
        critical_section::with(|_| {
            let v = r.chep(0);
            let w = chep::with_config(v, chep::config(0, CONTROL, status_out));
            r.set_chep(0, chep::to_stat_tx(chep::to_stat_rx(w, v, rx), v, tx));
        })
    }

    /// Waits for `done(CHEP0R)`; a SETUP or a bus reset ends the wait first.
    async fn wait(&self, resets: u8, done: impl Fn(u32) -> bool) -> Result<(), Ep0Error> {
        poll_fn(|cx| {
            self.ep.in_waker.register(cx.waker());
            self.ep.out_waker.register(cx.waker());
            self.bus.waker.register(cx.waker());
            if self.bus.resets() != resets {
                return Poll::Ready(Err(Ep0Error::Reset));
            }
            let v = self.regs.chep(0);
            // a SETUP the handler hasn't seen yet also left STAT at NAK: look at the register as well
            if self.bus.peek() & event::SETUP != 0
                || v & (bits::VTRX | bits::SETUP) == bits::VTRX | bits::SETUP
            {
                return Poll::Ready(Err(Ep0Error::Setup));
            }
            if done(v) {
                Poll::Ready(Ok(()))
            } else {
                Poll::Pending
            }
        })
        .await
    }

    /// Waits for the next SETUP packet. It is read after the R6 wait (ES0548 2.16.1 holds for SETUP as for any
    /// OUT transaction); STATRX and STATTX are NAK until the next call here arms them.
    pub async fn setup(&mut self) -> Setup {
        loop {
            poll_fn(|cx| {
                self.bus.waker.register(cx.waker());
                if self.bus.take(event::SETUP) != 0 {
                    Poll::Ready(())
                } else {
                    Poll::Pending
                }
            })
            .await;
            let r = &self.regs;
            r.delay_ns(OUT_PACKET_DELAY_NS);
            if bd_count(r.sram(1)) != 8 {
                // not a SETUP's 8 bytes: wait for the host's retry
                self.idle();
                continue;
            }
            let mut raw = [0u8; 8];
            read_packet(r, self.buffer[1], &mut raw);
            // a second SETUP that overwrote the buffer while it was read raised the event again: read that one
            if self.bus.peek() & event::SETUP != 0 {
                continue;
            }
            return Setup::parse(&raw);
        }
    }

    /// Sends one packet of a control IN data stage (at most the max packet size) and waits until the host has it.
    /// Not the `last` stage: OUT STALL, a host that ends the data stage early gets a STALL; the last: OUT NAK.
    /// After the last, [Ep0::status_out] takes the status stage.
    pub async fn data_in(&mut self, data: &[u8], last: bool) -> Result<(), Ep0Error> {
        let resets = self.bus.resets();
        let r = &self.regs;
        critical_section::with(|_| {
            // R3: the interrupt flag of the packet before cleared first, then the buffer, then COUNT, then VALID
            let v = r.chep(0);
            if v & bits::VTTX != 0 {
                r.set_chep(0, chep::clear_vttx(v));
            }
            write_packet(r, self.buffer[0], data);
            r.set_sram(
                0,
                self.buffer[0] as u32 | ((data.len() as u32) << bd::COUNT_SHIFT),
            );
        });
        self.arm(
            if last { Stat::Nak } else { Stat::Stall },
            Stat::Valid,
            false,
        );
        self.wait(resets, |v| chep::stat_tx(v) != Stat::Valid).await
    }

    /// The status stage of a control read: a zero-length OUT, which STATUS_OUT makes the only packet the
    /// endpoint takes (one with data gets a STALL). Waits until it is in. The interrupt handler arms EP0 for the
    /// next SETUP when it sees the status packet, so no SETUP waits for this task.
    pub async fn status_out(&mut self) -> Result<(), Ep0Error> {
        let resets = self.bus.resets();
        self.arm(Stat::Valid, Stat::Nak, true);
        // done: STATRX NAK after the packet, or STATUS_OUT already cleared by the handler
        self.wait(resets, |v| {
            chep::stat_rx(v) != Stat::Valid || v & bits::EPKIND == 0
        })
        .await
    }

    /// Receives one packet of a control OUT data stage into `buf` and gives its length. Not the `last` stage: IN
    /// STALL; the last: IN NAK, then [Ep0::status_in].
    pub async fn data_out(&mut self, buf: &mut [u8], last: bool) -> Result<usize, Ep0Error> {
        let resets = self.bus.resets();
        self.arm(
            Stat::Valid,
            if last { Stat::Nak } else { Stat::Stall },
            false,
        );
        self.wait(resets, |v| chep::stat_rx(v) != Stat::Valid)
            .await?;
        let r = &self.regs;
        r.delay_ns(OUT_PACKET_DELAY_NS);
        let len = bd_count(r.sram(1));
        if len > buf.len() {
            return Err(Ep0Error::BufferOverflow);
        }
        read_packet(r, self.buffer[1], &mut buf[..len]);
        // a SETUP lands in the same buffer: if one came meanwhile, these bytes may be its
        if self.bus.peek() & event::SETUP != 0 {
            return Err(Ep0Error::Setup);
        }
        Ok(len)
    }

    /// The status stage of a control write or a request without data: a zero-length IN, with OUT STALL meanwhile
    /// (a SETUP is still taken, see [Ep0::stall]). Waits until the host has it; SET_ADDRESS applies its address
    /// after this (R14).
    pub async fn status_in(&mut self) -> Result<(), Ep0Error> {
        let resets = self.bus.resets();
        let r = &self.regs;
        critical_section::with(|_| {
            let v = r.chep(0);
            if v & bits::VTTX != 0 {
                r.set_chep(0, chep::clear_vttx(v));
            }
            r.set_sram(0, self.buffer[0] as u32);
        });
        self.arm(Stat::Stall, Stat::Valid, false);
        self.wait(resets, |v| chep::stat_tx(v) != Stat::Valid).await
    }

    /// Both directions STALL: the request is not supported (a SETUP is still taken, RM0444 §37.6.7: "If the
    /// control endpoint/channel is defined as STALL in the receive direction, then the SETUP packet is accepted
    /// anyway").
    pub fn stall(&mut self) {
        self.arm(Stat::Stall, Stat::Stall, false);
    }

    /// Between transfers: OUT VALID for the next SETUP, IN NAK.
    pub fn idle(&mut self) {
        self.arm(Stat::Valid, Stat::Nak, false);
    }
}
