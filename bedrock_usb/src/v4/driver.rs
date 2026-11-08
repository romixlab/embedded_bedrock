//! The peripheral as a whole: start-up, the interrupt handler, enabling and halting endpoints, the device
//! address. Endpoint data moves in [super::endpoint] and [super::ep0].

use core::cell::Cell;
use core::future::poll_fn;
use core::marker::PhantomData;
use core::task::Poll;

use super::chep::{self, Stat};
use super::endpoint::{BulkIn, BulkOut, InterruptIn};
use super::ep0::Ep0;
use super::regs::{Regs, bcdr, bd, chep as bits, cntr, daddr, istr};
use super::state::{EpState, Queue, RxQueue, State, StateRef, TxQueue, event};
use crate::buffer::Mode;
use crate::layout::{Layout, RegMode, RegType};
use crate::{Descriptors, Direction, EndpointAddress};

/// Interrupts the driver unmasks: transfers, bus reset, suspend and wake-up. SOF and ESOF only for a state made
/// with [State::with_sof] ([SOF_INTERRUPTS]).
pub const INTERRUPTS: u32 = cntr::CTRM | cntr::RST_DCONM | cntr::SUSPM | cntr::WKUPM;

/// The interrupts SOF timestamps add: SOF, and ESOF to count missed SOFs. ESOF is masked again while the bus is
/// suspended, where it would come every millisecond (RM0444 Rev 6 §37.5.7: the L2RES interval "can be timed
/// using ESOF interrupts, occurring with a 1 ms period when the system clock is running at nominal frequency").
pub const SOF_INTERRUPTS: u32 = cntr::SOFM | cntr::ESOFM;

/// tSTARTUP, "USB transceiver startup time", 1 µs max (DS13560 Rev 6) (R9).
pub const T_STARTUP_NS: u32 = 1_000;

/// ES0548 Rev 4 §2.16.1 "Buffer description table update completes after CTR interrupt triggers" (STM32G0B1,
/// rev A, Z/3 and Y/4: workaround A): "During OUT transfers, the correct transfer interrupt (CTR) is triggered a
/// little before the last USB SRAM accesses have completed. If the software responds quickly to the interrupt,
/// the full buffer contents may not be correct." Workaround: "Software should ensure that a small delay is
/// included before accessing the SRAM contents. This delay should be 800 ns in Full Speed mode" (R6, check USB-5).
/// Every read of an OUT packet's count or data, EP0's SETUP included, waits this long after seeing the packet.
pub const OUT_PACKET_DELAY_NS: u32 = 800;

/// An endpoint can't move data.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum EndpointError {
    /// Not enabled: not configured yet, or a bus reset or SET_CONFIGURATION 0 disabled it.
    Disabled,
    /// The packet doesn't fit the buffer given (OUT), or is longer than the max packet size (IN).
    BufferOverflow,
}

/// The usb_v4 driver of device `D` on registers `R`.
///
/// Created once, started with [Driver::init], connected to the bus with [Driver::connect]. The interrupt handler
/// is [Driver::on_interrupt] (or [super::on_interrupt] from a vector, after [Driver::register]).
pub struct Driver<D: Descriptors, R: Regs + Copy> {
    regs: R,
    state: StateRef,
    /// Endpoint registers handed out as typed endpoints, bit n for register n, bit n + 8 for its IN side.
    taken: Cell<u16>,
    _d: PhantomData<D>,
}

impl<D: Descriptors, R: Regs + Copy> Driver<D, R> {
    /// The driver on `regs` with the shared `state` ([crate::state!] sizes it for `D`).
    pub fn new<const N: usize>(regs: R, state: &'static State<N>) -> Self {
        const {
            assert!(
                N == D::LAYOUT.count as usize,
                "State<N>: N is D::LAYOUT.count, use bedrock_usb::state!"
            )
        };
        Driver {
            regs,
            state: state.as_ref(),
            taken: Cell::new(0),
            _d: PhantomData,
        }
    }

    pub fn regs(&self) -> R {
        self.regs
    }

    /// The bus share of the state: events, the run task's waker, the status.
    pub(crate) fn bus(&self) -> &'static super::state::BusState {
        self.state.bus
    }

    /// The layout, as a `'static` reference.
    pub const fn layout() -> &'static Layout {
        const { &D::LAYOUT }
    }

    /// Powers the transceiver up and takes the peripheral out of reset (R9), then sets up EP0 and the buffer table
    /// as a bus reset does. RM0444 §37.5.2: after PDWN "it is thus necessary to wait this time \[tSTARTUP\], before
    /// removing the reset condition on the USB part (by clearing the USBRST bit ...). Clearing the ISTR register
    /// removes any spurious pending interrupt before any other macrocell operation is enabled." The RCC clock and
    /// the 48 MHz USB kernel clock are the caller's.
    pub fn init(&self) {
        let r = &self.regs;
        r.set_cntr(cntr::USBRST); // PDWN cleared: transceiver on, peripheral still in reset
        r.delay_ns(T_STARTUP_NS);
        r.set_cntr(interrupts(self.state)); // USBRST cleared
        r.set_istr(0);
        bus_reset(r, Self::layout(), self.state);
        // a reset event before the host has reset the bus is no news to the firmware
        self.state.bus.take(event::RESET);
    }

    /// The DP pull-up on: the host sees the device and resets the bus.
    pub fn connect(&self) {
        critical_section::with(|_| self.regs.set_bcdr(self.regs.bcdr() | bcdr::DPPU));
    }

    /// The DP pull-up off: the host sees the device leave.
    pub fn disconnect(&self) {
        critical_section::with(|_| self.regs.set_bcdr(self.regs.bcdr() & !bcdr::DPPU));
    }

    /// The interrupt handler, for the vector (or for a test driving a simulated chip).
    pub fn on_interrupt(&self) {
        interrupt(&self.regs, Self::layout(), self.state)
    }

    /// The last SOF timestamped ([State::with_sof]).
    pub fn sof(&self) -> Option<super::Sof> {
        self.state.bus.sof()
    }

    /// Bus events raised since they were last taken, cleared ([event] bits in `mask`).
    pub fn take_events(&self, mask: u8) -> u8 {
        self.state.bus.take(mask)
    }

    /// Waits until one of the [event]s in `mask` is raised and takes them.
    pub async fn wait_events(&self, mask: u8) -> u8 {
        poll_fn(|cx| {
            self.state.bus.waker.register(cx.waker());
            match self.state.bus.take(mask) {
                0 => Poll::Pending,
                e => Poll::Ready(e),
            }
        })
        .await
    }

    /// The device address, written after the status stage of SET_ADDRESS (R14: USB 2.0 §9.4.6, "The USB device
    /// does not change its device address until after the Status stage of this request is completed
    /// successfully").
    pub fn set_address(&self, address: u8) {
        self.regs
            .set_daddr(daddr::EF | (address as u32 & daddr::ADD));
    }

    /// The control endpoint.
    pub fn ep0(&self) -> Ep0<R> {
        Ep0::new(
            self.regs,
            self.state.bus,
            &self.state.eps[0],
            Self::layout().regs[0].buffer,
        )
    }

    fn take(&self, number: u8, direction: Direction) {
        let bit = 1u16
            << (number as u16
                + if matches!(direction, Direction::In) {
                    8
                } else {
                    0
                });
        assert!(self.taken.get() & bit == 0, "endpoint handed out twice");
        self.taken.set(self.taken.get() | bit);
    }

    /// Bulk OUT endpoint `EP` with buffering `B`; the build fails when `D` lacks it.
    pub fn bulk_out<const EP: u8, B: Mode>(&self) -> BulkOut<R, EP, B> {
        const {
            assert!(
                D::DEVICE.has(Direction::Out, EP, B::BUFFERING),
                "no such OUT endpoint with this buffering"
            );
            assert!(
                kind_is(&D::LAYOUT, EP, RegType::Bulk),
                "not a bulk endpoint"
            );
        };
        self.take(EP, Direction::Out);
        let reg = &Self::layout().regs[EP as usize];
        BulkOut::new(self.regs, &self.state.eps[EP as usize], reg)
    }

    /// Bulk IN endpoint `EP` with buffering `B`; the build fails when `D` lacks it.
    pub fn bulk_in<const EP: u8, B: Mode>(&self) -> BulkIn<R, EP, B> {
        const {
            assert!(
                D::DEVICE.has(Direction::In, EP, B::BUFFERING),
                "no such IN endpoint with this buffering"
            );
            assert!(
                kind_is(&D::LAYOUT, EP, RegType::Bulk),
                "not a bulk endpoint"
            );
        };
        self.take(EP, Direction::In);
        let reg = &Self::layout().regs[EP as usize];
        BulkIn::new(self.regs, &self.state.eps[EP as usize], reg)
    }

    /// Interrupt IN endpoint `EP` (single-buffered); the build fails when `D` lacks it.
    pub fn interrupt_in<const EP: u8>(&self) -> InterruptIn<R, EP> {
        const {
            assert!(
                D::DEVICE.has(Direction::In, EP, crate::Buffering::Single),
                "no such IN endpoint, single-buffered"
            );
            assert!(
                kind_is(&D::LAYOUT, EP, RegType::Interrupt),
                "not an interrupt endpoint"
            );
        };
        self.take(EP, Direction::In);
        let reg = &Self::layout().regs[EP as usize];
        InterruptIn::new(self.regs, &self.state.eps[EP as usize], reg)
    }

    /// Enables (SET_CONFIGURATION, SET_INTERFACE) or disables a class endpoint, with DATA0 and, double-buffered,
    /// the first buffer (R12: USB 2.0 §9.1.1.5, "Configuring a device or changing an alternate setting causes all
    /// of the status and configuration values associated with endpoints in the affected interfaces to be set to
    /// their default values. This includes setting the data toggle of any endpoint using data toggles to the value
    /// DATA0."). An OUT endpoint is enabled VALID, ready for a packet; an IN endpoint NAK until there is data.
    /// Endpoints `D` lacks are ignored.
    pub fn set_enabled(&self, address: EndpointAddress, enabled: bool) {
        let Some((n, mode)) = Self::class_reg(address) else {
            return;
        };
        let r = &self.regs;
        let ep = &self.state.eps[n];
        let dir = address.direction();
        critical_section::with(|cs| match mode {
            RegMode::Double { .. } => {
                let stat = match (enabled, dir) {
                    (false, _) => Stat::Disabled,
                    (true, Direction::Out) => Stat::Valid,
                    (true, Direction::In) => Stat::Nak,
                };
                reset_double(r, Self::layout(), n, dir, stat, ep, cs);
            }
            _ => {
                let target = match (enabled, dir) {
                    (false, _) => Stat::Disabled,
                    (true, Direction::Out) => Stat::Valid,
                    (true, Direction::In) => Stat::Nak,
                };
                set_single(r, n, dir, |_| target, enabled);
            }
        });
        wake(ep, dir);
    }

    /// SET_FEATURE / CLEAR_FEATURE(ENDPOINT_HALT) on a class endpoint. Clearing always resets the data toggle to
    /// DATA0, halted or not (R12: USB 2.0 §9.4.5, "regardless of whether an endpoint has the Halt feature set, a
    /// ClearFeature(ENDPOINT_HALT) request always results in the data toggle being reinitialized to DATA0"). A
    /// disabled endpoint stays disabled.
    pub fn set_stalled(&self, address: EndpointAddress, stalled: bool) {
        let Some((n, mode)) = Self::class_reg(address) else {
            return;
        };
        let r = &self.regs;
        let ep = &self.state.eps[n];
        let dir = address.direction();
        critical_section::with(|cs| {
            if stat_of(r.chep(n), dir) == Stat::Disabled {
                return;
            }
            match mode {
                RegMode::Double { .. } => {
                    let stat = match (stalled, dir) {
                        (true, _) => Stat::Stall,
                        (false, Direction::Out) => Stat::Valid,
                        (false, Direction::In) => Stat::Nak,
                    };
                    reset_double(r, Self::layout(), n, dir, stat, ep, cs);
                }
                _ => {
                    let unstall = match dir {
                        Direction::Out => Stat::Valid,
                        Direction::In => Stat::Nak,
                    };
                    // checked once above: a transfer can complete between a read and a write of the loop, and
                    // the loop still has to land on the target
                    set_single(
                        r,
                        n,
                        dir,
                        |now| match (stalled, now) {
                            (true, _) => Stat::Stall,
                            (false, Stat::Stall) => unstall,
                            (false, now) => now,
                        },
                        !stalled,
                    );
                }
            }
        });
        wake(ep, dir);
    }

    /// Whether a class endpoint is halted (GET_STATUS).
    pub fn is_stalled(&self, address: EndpointAddress) -> bool {
        match Self::class_reg(address) {
            Some((n, _)) => stat_of(self.regs.chep(n), address.direction()) == Stat::Stall,
            None => false,
        }
    }

    fn class_reg(address: EndpointAddress) -> Option<(usize, RegMode)> {
        let n = address.number() as usize;
        if n == 0 || n >= D::LAYOUT.count as usize {
            return None;
        }
        let mode = D::LAYOUT.regs[n].mode;
        let has = match (mode, address.direction()) {
            (RegMode::Unused, _) => false,
            (RegMode::Single { out_size, .. }, Direction::Out) => out_size > 0,
            (RegMode::Single { in_size, .. }, Direction::In) => in_size > 0,
            (RegMode::Double { direction, .. }, d) => direction == d,
        };
        has.then_some((n, mode))
    }
}

impl<D: Descriptors> Driver<D, super::regs::Mmio> {
    /// Makes this driver the one [super::on_interrupt] serves. Call it before unmasking the interrupt.
    pub fn register(&self) {
        super::register(super::Active {
            regs: self.regs,
            layout: Self::layout(),
            state: self.state,
        })
    }
}

const fn kind_is(layout: &Layout, n: u8, t: RegType) -> bool {
    (n as usize) < layout.regs.len()
        && matches!(
            (layout.regs[n as usize].reg_type, t),
            (RegType::Bulk, RegType::Bulk) | (RegType::Interrupt, RegType::Interrupt)
        )
}

fn stat_of(r: u32, dir: Direction) -> Stat {
    match dir {
        Direction::Out => chep::stat_rx(r),
        Direction::In => chep::stat_tx(r),
    }
}

fn wake(ep: &EpState, dir: Direction) {
    match dir {
        Direction::Out => ep.out_waker.wake(),
        Direction::In => ep.in_waker.wake(),
    }
}

/// Moves a single-buffered direction to `target(current)`, retrying until the register holds it: the hardware can
/// change STAT between the read and the write (a transfer completes), R1. `dtog0` also resets the data toggle.
fn set_single<R: Regs>(
    r: &R,
    n: usize,
    dir: Direction,
    target: impl Fn(Stat) -> Stat,
    dtog0: bool,
) {
    loop {
        let v = r.chep(n);
        let now = stat_of(v, dir);
        let want = target(now);
        let (dtog, w) = match dir {
            Direction::Out => (
                v & bits::DTOGRX != 0,
                chep::to_stat_rx(chep::invariant(v), v, want),
            ),
            Direction::In => (
                v & bits::DTOGTX != 0,
                chep::to_stat_tx(chep::invariant(v), v, want),
            ),
        };
        let reset = dtog0 && dtog;
        if now == want && !reset {
            return;
        }
        let w = match (dir, reset) {
            (Direction::Out, true) => w | bits::DTOGRX,
            (Direction::In, true) => w | bits::DTOGTX,
            _ => w,
        };
        r.set_chep(n, w);
    }
}

/// Starts a double-buffered register over: DATA0, buffer 0 first, nothing queued, then STAT `stat`.
///
/// Disabled first, so the peripheral can't move DTOG while the selectors are set; RM0444 §37.5.3: "The
/// application software is responsible for DTOG and SW_BUF bits initialization according to the first buffer to
/// be used". The selectors and STAT go in separate writes (R4); see [release].
fn reset_double<R: Regs>(
    r: &R,
    layout: &Layout,
    n: usize,
    dir: Direction,
    stat: Stat,
    ep: &EpState,
    cs: critical_section::CriticalSection,
) {
    set_single(r, n, dir, |_| Stat::Disabled, false);
    let v = r.chep(n);
    let w = match dir {
        Direction::In => {
            // DTOG (DTOGTX) 0 and SW_BUF (DTOGRX) 0: no buffer for the peripheral until the first packet; VTTX of a
            // packet from before is dropped
            let w = chep::to_dtog_tx(chep::clear_vttx(v), v, false);
            chep::to_dtog_rx(w, v, false)
        }
        Direction::Out => {
            for b in 0..2 {
                let i = 2 * n + b;
                r.set_sram(i, layout.regs[n].bd[b]);
            }
            // DTOG (DTOGRX) 0 and SW_BUF (DTOGTX) 1: the peripheral receives into buffer 0
            let w = chep::to_dtog_rx(chep::clear_vtrx(v), v, false);
            chep::to_dtog_tx(w, v, true)
        }
    };
    r.set_chep(n, w);
    if stat != Stat::Disabled {
        set_single(r, n, dir, |_| stat, false);
    }
    ep.set_queue(
        cs,
        match dir {
            Direction::In => Queue::In(TxQueue::new()),
            Direction::Out => Queue::Out(RxQueue::new()),
        },
    );
}

/// Hands the buffer the software holds to the peripheral: toggles SW_BUF, then, in a write of its own, turns a
/// NAK into VALID.
///
/// R4: bedrock_usb never writes STAT and SW_BUF in one write. RM0444 §37.5.3 says STAT stays VALID and the
/// endpoint is only "masked as 10 (NAK) when a buffer conflict" is seen; embassy-stm32 (`usb/usb.rs`, from
/// embassy-rs/embassy#7275) says "Hardware sets STAT to NAK when a transfer leaves the selectors equal, and
/// toggling SW_BUF then restores VALID by itself. Toggling STAT in that same write would turn it back into NAK."
/// Two writes are right under both readings; USBD-7 settles which one the silicon does.
pub(crate) fn release<R: Regs>(r: &R, n: usize, dir: Direction) {
    let v = r.chep(n);
    let sw_buf = match dir {
        Direction::In => bits::DTOGRX,
        Direction::Out => bits::DTOGTX,
    };
    r.set_chep(n, chep::invariant(v) | sw_buf);
    let v = r.chep(n);
    if stat_of(v, dir) == Stat::Nak {
        let w = match dir {
            Direction::In => chep::to_stat_tx(chep::invariant(v), v, Stat::Valid),
            Direction::Out => chep::to_stat_rx(chep::invariant(v), v, Stat::Valid),
        };
        r.set_chep(n, w);
    }
}

/// What a bus reset leaves (R8, RM0444 §37.5.2): "the USB function must be enabled, having as USB address 0,
/// implementing only the default control endpoint ... This is accomplished by setting the enable function (EF)
/// bit of the USB_DADDR register and initializing the CHEP0R register and its related packet buffers
/// accordingly", "within 10 ms from the end of the reset sequence". All values come from the const layout.
fn bus_reset<R: Regs>(r: &R, layout: &Layout, st: StateRef) {
    critical_section::with(|cs| {
        for n in 0..layout.count as usize {
            let reg = &layout.regs[n];
            r.set_sram(2 * n, reg.bd[0]);
            r.set_sram(2 * n + 1, reg.bd[1]);
            let (utype, kind) = match reg.mode {
                RegMode::Unused => continue,
                RegMode::Double { .. } => (reg.reg_type.utype(), true),
                RegMode::Single { .. } => (reg.reg_type.utype(), false),
            };
            // the reset cleared every bit but VTRX and VTTX ("kept unchanged to avoid missing a correct packet
            // notification immediately followed by an USB reset event", §37.6.7); a packet from before the reset
            // is no one's any more, so they are cleared too
            let v = r.chep(n);
            let mut w = chep::with_config(v, chep::config(n as u8, utype, kind))
                & !(bits::VTRX | bits::VTTX);
            w = chep::to_dtog_rx(chep::to_dtog_tx(w, v, false), v, false);
            w = if n == 0 {
                // EP0 idle: ready for a SETUP, nothing to send
                chep::to_stat_tx(chep::to_stat_rx(w, v, Stat::Valid), v, Stat::Nak)
            } else {
                chep::to_stat_tx(chep::to_stat_rx(w, v, Stat::Disabled), v, Stat::Disabled)
            };
            r.set_chep(n, w);
            st.eps[n].set_queue(
                cs,
                match reg.mode {
                    RegMode::Double {
                        direction: Direction::In,
                        ..
                    } => Queue::In(TxQueue::new()),
                    RegMode::Double {
                        direction: Direction::Out,
                        ..
                    } => Queue::Out(RxQueue::new()),
                    _ => Queue::None,
                },
            );
        }
        r.set_daddr(daddr::EF);
        st.bus.reset(cs);
    });
    for ep in st.eps {
        ep.wake_both();
    }
    st.bus.waker.wake();
}

/// The interrupts unmasked for state `st`.
fn interrupts(st: StateRef) -> u32 {
    if st.bus.sof_enabled() {
        INTERRUPTS | SOF_INTERRUPTS
    } else {
        INTERRUPTS
    }
}

/// The interrupt handler of a device with `layout` on `r`.
///
/// - SOF, first: with [State::with_sof], the firmware's clock read and FNR's frame number stored ([super::Sof]),
///   before any other work, so the timestamp is the SOF plus the interrupt latency and nothing else of this
///   handler. ESOF: counted as a missed SOF while FNR's LCK is set.
/// - RST_DCON: the bus reset here, so the 10 ms of R8 hold whatever the executor does.
/// - CTR, for each register with a transfer done: VTRX / VTTX cleared with an invariant write (R1) and the
///   waiting task woken; a SETUP on EP0 raises [event::SETUP]. Double-buffered registers hand their next buffer
///   to the peripheral here. Data is copied by the task, not here.
/// - SUSP: SUSPEN set (RM0444 §37.5.7 step 1, "This action activates the suspend mode within the USB peripheral");
///   lowering power further (SUSPRDY, clocks) is the firmware's. WKUP: SUSPEN and SUSPRDY cleared.
///
/// Each flag is cleared with a write of 0 to it and 1 everywhere else (`rc_w0`). Returns at once when nothing of
/// the USB is pending, so a vector shared with UCPD costs a few cycles.
pub fn interrupt<R: Regs>(r: &R, layout: &Layout, st: StateRef) {
    let flags = r.istr();
    if flags & (istr::RST_DCON | istr::CTR | istr::SUSP | istr::WKUP | istr::ESOF | istr::SOF) == 0
    {
        return;
    }
    if flags & istr::SOF != 0 {
        // the flag is set whether or not SOFM is: cleared either way, so a shared vector doesn't come back here
        if let Some(now) = st.bus.sof_clock {
            let t = now();
            st.bus.sof_seen(t, r.fnr());
        }
        r.set_istr(!istr::SOF);
    }
    if flags & istr::RST_DCON != 0 {
        r.set_istr(!istr::RST_DCON);
        bus_reset(r, layout, st);
        if st.bus.sof_enabled() {
            // a reset out of suspend: ESOF back on
            r.set_cntr(r.cntr() | SOF_INTERRUPTS);
        }
    }
    loop {
        let flags = r.istr();
        if flags & istr::CTR == 0 {
            break;
        }
        let n = (flags & istr::IDN) as usize;
        if n >= st.eps.len() {
            // a register the layout doesn't use: clear it so the loop ends
            let v = r.chep(n);
            r.set_chep(n, chep::invariant(v) & !(bits::VTRX | bits::VTTX));
            continue;
        }
        transfer_done(r, n, st);
    }
    let mut events = 0;
    if flags & istr::SUSP != 0 {
        let off = if st.bus.sof_enabled() { cntr::ESOFM } else { 0 };
        r.set_cntr((r.cntr() | cntr::SUSPEN) & !off);
        r.set_istr(!istr::SUSP);
        events |= event::SUSPEND;
    }
    if flags & istr::WKUP != 0 {
        r.set_cntr((r.cntr() & !(cntr::SUSPEN | cntr::SUSPRDY)) | (interrupts(st) & cntr::ESOFM));
        r.set_istr(!istr::WKUP);
        events |= event::RESUME;
    }
    if flags & istr::ESOF != 0 {
        // LSOF and LCK are written at the ESOF, so FNR read now is this ESOF's
        st.bus.esof_seen(r.fnr());
        r.set_istr(!istr::ESOF);
        events |= event::ESOF;
    }
    if events != 0 {
        critical_section::with(|cs| st.bus.raise(cs, events));
        st.bus.waker.wake();
    }
}

fn transfer_done<R: Regs>(r: &R, n: usize, st: StateRef) {
    let ep = &st.eps[n];
    critical_section::with(|cs| {
        let v = r.chep(n);
        match ep.queue(cs) {
            Queue::In(mut q) if v & bits::VTTX != 0 => {
                r.set_chep(n, chep::clear_vttx(v));
                if q.sent() && matches!(chep::stat_tx(v), Stat::Valid | Stat::Nak) {
                    release(r, n, Direction::In);
                }
                ep.set_queue(cs, Queue::In(q));
                ep.in_waker.wake();
            }
            Queue::Out(mut q) if v & bits::VTRX != 0 => {
                r.set_chep(n, chep::clear_vtrx(v));
                if q.received() && matches!(chep::stat_rx(v), Stat::Valid | Stat::Nak) {
                    release(r, n, Direction::Out);
                }
                ep.set_queue(cs, Queue::Out(q));
                ep.out_waker.wake();
            }
            _ => {
                // SETUP is frozen while VTRX is 1 (§37.6.7), so it is read before VTRX is cleared
                if n == 0 && v & bits::VTRX != 0 && v & bits::SETUP != 0 {
                    st.bus.raise(cs, event::SETUP);
                }
                let mut w = chep::invariant(v);
                if n == 0
                    && v & (bits::VTRX | bits::SETUP | bits::EPKIND) == bits::VTRX | bits::EPKIND
                {
                    // the status OUT of a control read is in: STATUS_OUT off and OUT VALID again right here, so
                    // the host's next SETUP is taken whatever the task does (the hardware left STATRX at NAK, and
                    // a SETUP to a NAK endpoint gets no answer)
                    w = chep::to_stat_rx(
                        chep::with_config(v, chep::config(0, RegType::Control.utype(), false)),
                        v,
                        Stat::Valid,
                    );
                }
                if v & bits::VTRX != 0 {
                    w &= !bits::VTRX;
                }
                if v & bits::VTTX != 0 {
                    w &= !bits::VTTX;
                }
                r.set_chep(n, w);
                if v & bits::VTRX != 0 {
                    ep.out_waker.wake();
                }
                if v & bits::VTTX != 0 {
                    ep.in_waker.wake();
                }
            }
        }
    });
    if n == 0 {
        st.bus.waker.wake();
    }
}

/// Count field of a buffer descriptor word.
pub(crate) const fn bd_count(word: u32) -> usize {
    ((word & bd::COUNT) >> bd::COUNT_SHIFT) as usize
}
