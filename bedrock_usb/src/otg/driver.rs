//! The core as a whole: start-up, the bus reset, the interrupt handler, enabling and halting endpoints, the
//! device address. Endpoint data moves in [super::endpoint] and [super::ep0].

use core::cell::Cell;
use core::future::poll_fn;
use core::marker::PhantomData;
use core::task::Poll;

use super::endpoint::{BulkIn, BulkOut, InterruptIn};
use super::ep0::Ep0;
use super::layout::{EpType, Layout};
use super::regs::{
    Regs, ctl, daint, dcfg, dctl, dsts, epint, fsiz, gahbcfg, gccfg, gint, gotgctl, grstctl,
    grxsts, gusbcfg, offset, tsiz,
};
use super::state::{OutQueue, State, StateRef};
use super::{OtgDescriptors, Phy};
use crate::buffer::Mode;
use crate::v4::state::{BusState, event};
use crate::{Direction, EndpointAddress, Speed};

/// Core interrupts the driver unmasks (§62.15.3 step 4, without early suspend and SOF): bus reset, enumeration
/// done, suspend, wake-up, the receive FIFO and the endpoints.
pub const INTERRUPTS: u32 = gint::USBRST
    | gint::ENUMDNE
    | gint::USBSUSP
    | gint::WKUPINT
    | gint::RXFLVL
    | gint::IEPINT
    | gint::OEPINT;

/// Endpoint interrupts the driver unmasks (§62.15.6 "Endpoint initialization on USB reset", step 2): "STUPM = 1
/// in OTG_DOEPMSK, XFRCM = 1 in OTG_DOEPMSK, XFRCM = 1 in OTG_DIEPMSK, TOM = 1 in OTG_DIEPMSK". INEPNE and
/// EPDISD stay masked: the driver polls them while it stops an endpoint, and the interrupt handler leaves them.
pub const DIEPMSK: u32 = epint::XFRC | epint::TOC;
pub const DOEPMSK: u32 = epint::XFRC | epint::STUP;

/// GUSBCFG.TRDT, RM0468 Rev 3 Table 540 "TRDT values": AHB frequency "30 - : 0x9" minimum. The H7's AHB runs
/// at 30 MHz or more whenever USB does; `embassy::new` checks it.
pub const TRDT: u32 = 0x9;

/// FDMOD: "After setting the force bit, the application must wait at least 25 ms before the change takes effect"
/// (§62.14.4).
pub const FORCE_MODE_US: u32 = 25_000;

/// Register reads a wait for the core gives up after: the core soft reset, a FIFO flush, an endpoint's NAK and
/// disable, the global OUT NAK. Each takes a few PHY clocks or at most a packet or (micro)frame
/// (embassy-usb-synopsys-otg measured 1.2 ms for the global OUT NAK where its interrupt handler popped the pattern;
/// [stop_out] pops it in place). A wait that runs out leaves the core as it is.
pub const SPIN_LIMIT: u32 = 200_000;

/// An endpoint can't move data.
pub use crate::v4::EndpointError;

/// The Synopsys OTG driver of device `D` on registers `R`.
///
/// Created once, started with [Driver::init], connected to the bus with [Driver::connect]. The interrupt handler
/// is [Driver::on_interrupt] (or [super::on_interrupt] from a vector, after [Driver::register]).
pub struct Driver<D: OtgDescriptors, R: Regs + Copy> {
    regs: R,
    state: StateRef,
    /// Endpoints handed out as typed endpoints: bit n OUT, bit 16 + n IN.
    taken: Cell<u32>,
    _d: PhantomData<D>,
}

/// Waits until `done` or [SPIN_LIMIT] reads; whether it came.
fn spin<R: Regs>(r: &R, offset: usize, done: impl Fn(u32) -> bool) -> bool {
    for _ in 0..SPIN_LIMIT {
        if done(r.read(offset)) {
            return true;
        }
    }
    false
}

/// Flushes transmit FIFO `n` ([grstctl::TXFNUM_ALL]: all of them). §62.14.5: "The application must write this bit
/// only after checking that the core is neither writing to the Tx FIFO nor reading from the Tx FIFO", so AHBIDL
/// first; the core clears TXFFLSH when it is done.
pub(crate) fn flush_tx<R: Regs>(r: &R, n: u32) {
    spin(r, offset::GRSTCTL, |v| v & grstctl::AHBIDL != 0);
    r.write(
        offset::GRSTCTL,
        grstctl::TXFFLSH | n << grstctl::TXFNUM_SHIFT,
    );
    spin(r, offset::GRSTCTL, |v| v & grstctl::TXFFLSH == 0);
}

/// Flushes the receive FIFO (§62.15.6 "Transfer Stop Programming for OUT endpoints" step 2: AHBIDL, then RXFFLSH,
/// then until it is 0).
fn flush_rx<R: Regs>(r: &R) {
    spin(r, offset::GRSTCTL, |v| v & grstctl::AHBIDL != 0);
    r.write(offset::GRSTCTL, grstctl::RXFFLSH);
    spin(r, offset::GRSTCTL, |v| v & grstctl::RXFFLSH == 0);
}

/// Writes `OTG_DIEPCTLx` / `OTG_DOEPCTLx` with the configuration read ([ctl::KEEP]) changed by `keep` and the
/// action bits `act`.
pub(crate) fn set_ctl<R: Regs>(r: &R, offset: usize, keep: impl Fn(u32) -> u32, act: u32) {
    let v = r.read(offset) & ctl::KEEP;
    r.write(offset, keep(v) | act);
}

/// Stops a transfer under way on IN endpoint `n` and empties its FIFO, per §62.15.6 "IN endpoint disable": "2.
/// The application must set the endpoint in NAK mode. SNAK = 1 ... 3. Wait for the INEPNE interrupt ... 4. Set
/// ... EPDIS = 1 ... SNAK = 1 ... 5. Assertion of the EPDISD interrupt ... indicates that the core has completely
/// disabled the specified endpoint ... 7. The application must flush the data in the endpoint transmit FIFO".
/// `stall` sets STALL in the same write as EPDIS (§62.15.6 "Stalling non-isochronous IN endpoints": "Disable the
/// IN endpoint to be stalled. Set the STALL bit as well."). Without it, an IN endpoint whose host left mid-transfer
/// keeps EPENA through every later reset and never sends again (embassy-usb-synopsys-otg found this on EP0).
pub(crate) fn stop_in<R: Regs>(r: &R, n: usize, stall: bool) {
    let c = offset::diepctl(n);
    let i = offset::diepint(n);
    if r.read(c) & ctl::EPENA != 0 {
        set_ctl(r, c, |v| v, ctl::SNAK);
        spin(r, i, |v| v & epint::INEPNE != 0);
        set_ctl(
            r,
            c,
            |v| if stall { v | ctl::STALL } else { v },
            ctl::SNAK | ctl::EPDIS,
        );
        spin(r, i, |v| v & epint::EPDISD != 0);
        r.write(i, epint::INEPNE | epint::EPDISD);
    }
    flush_tx(r, n as u32);
}

/// Disables the OUT endpoints in `mask` (bit n for endpoint n) that are enabled, per §62.15.6 "Disabling an OUT
/// endpoint": "1. Before disabling any OUT endpoint, the application must enable Global OUT NAK mode in the core.
/// – SGONAK = 1 in OTG_DCTL 2. Wait for the GONAKEFF interrupt (OTG_GINTSTS) 3. Disable the required OUT endpoint
/// by programming the following fields: – EPDIS = 1 in OTG_DOEPCTLx – SNAK = 1 in OTG_DOEPCTLx 4. Wait for the
/// EPDISD interrupt ... 5. The application must clear the Global OUT NAK bit". GONAKEFF only comes "When the
/// application pops the Global OUT NAK pattern word from the receive FIFO" ("Setting the global OUT NAK"), so
/// the wait empties the receive FIFO itself ([receive]) instead of counting on the interrupt handler, which the
/// caller's critical section holds off. Packets popped meanwhile go where the handler would put them.
pub(crate) fn stop_out<R: Regs>(r: &R, layout: &Layout, st: StateRef, mask: u32) {
    let mut enabled = 0u32;
    for n in 0..layout.count as usize {
        if mask & 1 << n != 0 && r.read(offset::doepctl(n)) & ctl::EPENA != 0 {
            enabled |= 1 << n;
        }
    }
    if enabled == 0 {
        return;
    }
    let d = r.read(offset::DCTL);
    r.write(offset::DCTL, d | dctl::SGONAK);
    for _ in 0..SPIN_LIMIT {
        if r.read(offset::GINTSTS) & gint::GONAKEFF != 0 {
            break;
        }
        receive(r, layout, st);
    }
    for n in 0..layout.count as usize {
        if enabled & 1 << n != 0 {
            set_ctl(r, offset::doepctl(n), |v| v, ctl::EPDIS | ctl::SNAK);
        }
    }
    for n in 0..layout.count as usize {
        if enabled & 1 << n != 0 {
            spin(r, offset::doepint(n), |v| v & epint::EPDISD != 0);
            r.write(offset::doepint(n), epint::EPDISD);
        }
    }
    let d = r.read(offset::DCTL);
    r.write(offset::DCTL, d | dctl::CGONAK);
}

/// Arms OUT endpoint `n` (not 0) for one packet of its max packet size at the current speed into the buffer its
/// queue gives, if it has a free one and isn't armed (§62.15.6 "Generic non-isochronous OUT data transfers":
/// "1. Program the OTG_DOEPTSIZx register for the transfer size and the corresponding packet count. 2. Program the
/// OTG_DOEPCTLx register with the endpoint characteristics, and set the EPENA and CNAK bits"). One packet per
/// transfer: the transfer completes at every packet, so each gets its own XFRC.
pub(crate) fn arm_out<R: Regs>(
    r: &R,
    layout: &Layout,
    st: StateRef,
    n: usize,
    cs: critical_section::CriticalSection,
) {
    let ep = &st.eps[n];
    let mut q = ep.out(cs);
    if q.to_arm().is_none() {
        return;
    }
    q.arm();
    ep.set_out(cs, q);
    let mps = layout.eps[n].out.max_packet_at(st.speed.borrow(cs).get());
    r.write(offset::doeptsiz(n), tsiz::one(mps as usize));
    set_ctl(r, offset::doepctl(n), |v| v, ctl::EPENA | ctl::CNAK);
}

impl<D: OtgDescriptors, R: Regs + Copy> Driver<D, R> {
    /// The driver on `regs` with the shared `state` ([crate::otg_state!] sizes it for `D`).
    pub fn new<const N: usize, const W: usize>(regs: R, state: &'static State<N, W>) -> Self {
        const {
            assert!(
                N == D::OTG.count as usize && W == D::OTG.out_words as usize,
                "State<N, W>: use bedrock_usb::otg_state!"
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

    /// The layout, as a `'static` reference.
    pub const fn layout() -> &'static Layout {
        const { &D::OTG }
    }

    /// Starts the core in device mode on `phy`, off the bus (SDIS) until [Driver::connect].
    ///
    /// The order is ST's (the PHY selected, then the core soft reset, then the mode): RM0468 §62.14.5 CSRST,
    /// "When the user changes the PHY, the corresponding clock for the PHY is selected and used in the PHY domain.
    /// Once a new clock is selected, the PHY domain has to be reset for proper operation", and "The software must
    /// also check that bit 31 in this register is set to 1 (AHB Master is Idle) before starting any operation".
    /// Then §62.15.3 "Device initialization": DCFG (speed, PFIVL 80 %), the interrupt masks, and the endpoints and
    /// FIFOs as a bus reset leaves them. VBUS isn't sensed: the B-session valid override says a host is there
    /// (GOTGCTL BVALOEN / BVALOVAL), the firmware connects when it wants to. The RCC clocks, the PHY's supply
    /// and, for ULPI, the pins and the ULPI clock are the caller's.
    pub fn init(&self, phy: Phy) {
        let r = &self.regs;
        r.write(offset::GAHBCFG, 0);
        let sel = match phy {
            Phy::InternalFullSpeed => gusbcfg::PHYSEL,
            Phy::Ulpi { .. } => 0,
        };
        r.write(offset::GUSBCFG, sel | TRDT << gusbcfg::TRDT_SHIFT);
        spin(r, offset::GRSTCTL, |v| v & grstctl::AHBIDL != 0);
        r.write(offset::GRSTCTL, grstctl::CSRST);
        spin(r, offset::GRSTCTL, |v| v & grstctl::CSRST == 0);
        spin(r, offset::GRSTCTL, |v| v & grstctl::AHBIDL != 0);
        // GCCFG PWRDWN: "0 = USB FS PHY disabled 1 = USB FS PHY enabled"
        let gccfg = match phy {
            Phy::InternalFullSpeed => gccfg::PWRDWN,
            Phy::Ulpi { .. } => 0,
        };
        r.write(offset::GCCFG, gccfg);
        r.write(offset::GOTGCTL, gotgctl::BVALOEN | gotgctl::BVALOVAL);
        r.write(
            offset::GUSBCFG,
            sel | TRDT << gusbcfg::TRDT_SHIFT | gusbcfg::FDMOD,
        );
        r.delay_us(FORCE_MODE_US);
        // the PHY clock on (STPPCLK / GATEHCLK off)
        r.write(offset::PCGCCTL, 0);
        let dspd = match phy {
            Phy::InternalFullSpeed => dcfg::DSPD_FULL_INTERNAL,
            Phy::Ulpi {
                high_speed: true, ..
            } => dcfg::DSPD_HIGH,
            Phy::Ulpi {
                high_speed: false, ..
            } => dcfg::DSPD_FULL_ULPI,
        };
        let xcvrdly = match phy {
            Phy::Ulpi { xcvrdly: true, .. } => dcfg::XCVRDLY,
            _ => 0,
        };
        r.write(offset::DCFG, dspd | xcvrdly);
        r.write(offset::DCTL, dctl::SDIS);
        r.write(offset::DIEPMSK, DIEPMSK);
        r.write(offset::DOEPMSK, DOEPMSK);
        bus_reset(r, Self::layout(), self.state);
        // a reset event before the host has reset the bus is no news to the firmware
        self.state.bus.take(event::RESET);
        r.write(offset::GINTSTS, !0);
        r.write(offset::GINTMSK, INTERRUPTS);
        r.write(offset::GAHBCFG, gahbcfg::GINT);
    }

    /// Off soft disconnect: "When this bit is cleared after a soft disconnect, the core generates a device connect
    /// event to the USB host" (§62.14.40 SDIS).
    pub fn connect(&self) {
        critical_section::with(|_| {
            let v = self.regs.read(offset::DCTL);
            self.regs.write(offset::DCTL, v & !dctl::SDIS);
        })
    }

    /// Soft disconnect: "The core generates a device disconnect event to the USB host". The host sees it after
    /// 2.5 µs at full speed, 125 µs at high speed, 1 ms + 2.5 µs while suspended (Table 541).
    pub fn disconnect(&self) {
        critical_section::with(|_| {
            let v = self.regs.read(offset::DCTL);
            self.regs.write(offset::DCTL, v | dctl::SDIS);
        })
    }

    /// The interrupt handler, for the vector (or for a test driving a simulated core).
    pub fn on_interrupt(&self) {
        interrupt(&self.regs, Self::layout(), self.state)
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

    /// The speed of the last enumeration.
    pub fn speed(&self) -> Speed {
        critical_section::with(|cs| self.state.speed.borrow(cs).get())
    }

    /// DCFG.DAD: "The application must program this field after every SetAddress control command" (§62.14.39),
    /// before the status stage ([crate::Backend::ADDRESS_BEFORE_STATUS]).
    pub fn set_address(&self, address: u8) {
        critical_section::with(|_| {
            let v = self.regs.read(offset::DCFG) & !dcfg::DAD;
            self.regs.write(
                offset::DCFG,
                v | ((address as u32) << dcfg::DAD_SHIFT) & dcfg::DAD,
            );
        })
    }

    /// The control endpoint.
    pub fn ep0(&self) -> Ep0<R> {
        Ep0::new(self.regs, self.state, Self::layout())
    }

    fn take(&self, number: u8, direction: Direction) {
        let bit = 1u32
            << (number as u32
                + if matches!(direction, Direction::In) {
                    16
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
                D::OTG.has(EP, Direction::Out, EpType::Bulk, B::BUFFERING),
                "no such bulk OUT endpoint with this buffering"
            )
        };
        self.take(EP, Direction::Out);
        BulkOut::new(self.regs, self.state, Self::layout())
    }

    /// Bulk IN endpoint `EP` with buffering `B`; the build fails when `D` lacks it.
    pub fn bulk_in<const EP: u8, B: Mode>(&self) -> BulkIn<R, EP, B> {
        const {
            assert!(
                D::OTG.has(EP, Direction::In, EpType::Bulk, B::BUFFERING),
                "no such bulk IN endpoint with this buffering"
            )
        };
        self.take(EP, Direction::In);
        BulkIn::new(self.regs, self.state, Self::layout())
    }

    /// Interrupt IN endpoint `EP` (single-buffered); the build fails when `D` lacks it.
    pub fn interrupt_in<const EP: u8>(&self) -> InterruptIn<R, EP> {
        const {
            assert!(
                D::OTG.has(
                    EP,
                    Direction::In,
                    EpType::Interrupt,
                    crate::Buffering::Single
                ),
                "no such interrupt IN endpoint, single-buffered"
            )
        };
        self.take(EP, Direction::In);
        InterruptIn::new(self.regs, self.state, Self::layout())
    }

    /// Enables (SET_CONFIGURATION, SET_INTERFACE) or disables a class endpoint, with DATA0 (R12). §62.15.6
    /// "Endpoint activation": "Program the characteristics of the required endpoint into the following fields of
    /// the OTG_DIEPCTLx register ... or the OTG_DOEPCTLx register ...: Maximum packet size, USB active endpoint =
    /// 1, Endpoint start data toggle (for interrupt and bulk endpoints), Endpoint type, Tx FIFO number"; the max
    /// packet size is the current speed's. An OUT endpoint is armed for its first packet, an IN endpoint NAKs
    /// until there is data.
    ///
    /// Disabling is §62.15.6 "Endpoint deactivation": "clear the USB active endpoint bit ... Once the endpoint is
    /// deactivated, the core ignores tokens addressed to that endpoint". An IN transfer under way is stopped and
    /// its FIFO flushed first ([stop_in]); an armed OUT endpoint is disabled under the global OUT NAK ([stop_out])
    /// and its queue dropped, so it can be armed again with a fresh EPENA. Endpoints `D` lacks are ignored.
    pub fn set_enabled(&self, address: EndpointAddress, enabled: bool) {
        let n = address.number() as usize;
        let layout = Self::layout();
        if n == 0 || n >= layout.count as usize {
            return;
        }
        let d = layout.dir(n, address.direction());
        if !d.is_used() {
            return;
        }
        let r = &self.regs;
        let st = self.state;
        match address.direction() {
            Direction::In => {
                let c = offset::diepctl(n);
                if enabled {
                    let mps = d.max_packet_at(self.speed());
                    critical_section::with(|_| {
                        stop_in(r, n, false);
                        r.write(
                            c,
                            mps as u32
                                | d.ep_type.eptyp()
                                | (n as u32) << ctl::TXFNUM_SHIFT
                                | ctl::USBAEP
                                | ctl::SD0PID
                                | ctl::SNAK,
                        );
                        let m = r.read(offset::DAINTMSK);
                        r.write(offset::DAINTMSK, m | daint::in_ep(n));
                    });
                } else {
                    critical_section::with(|_| {
                        stop_in(r, n, false);
                        set_ctl(r, c, |v| v & !(ctl::USBAEP | ctl::STALL), ctl::SNAK);
                    });
                }
                st.eps[n].in_waker.wake();
            }
            Direction::Out => {
                let c = offset::doepctl(n);
                if enabled {
                    let mps = d.max_packet_at(self.speed());
                    critical_section::with(|cs| {
                        r.write(
                            c,
                            mps as u32 | d.ep_type.eptyp() | ctl::USBAEP | ctl::SD0PID | ctl::SNAK,
                        );
                        st.eps[n].set_out(cs, OutQueue::new(d.packets));
                        let m = r.read(offset::DAINTMSK);
                        r.write(offset::DAINTMSK, m | daint::out_ep(n));
                        arm_out(r, layout, st, n, cs);
                    });
                } else {
                    critical_section::with(|cs| {
                        stop_out(r, layout, st, 1 << n);
                        set_ctl(r, c, |v| v & !(ctl::USBAEP | ctl::STALL), ctl::SNAK);
                        st.eps[n].set_out(cs, OutQueue::DISABLED);
                    });
                }
                st.eps[n].out_waker.wake();
            }
        }
    }

    /// SET_FEATURE / CLEAR_FEATURE(ENDPOINT_HALT) on a class endpoint; clearing always resets the data toggle to
    /// DATA0 (R12). IN: §62.15.6 "Stalling non-isochronous IN endpoints", a transfer under way stopped with STALL
    /// set and the FIFO flushed. OUT: STALL set ("The STALL bit always takes precedence over the NAK bit"); the
    /// endpoint stays armed and takes the next packet once the halt is cleared. Both before the status stage, as §62.15.6 asks: "the
    /// STALL bit must be set or cleared before the application sets up the status stage transfer on the control
    /// endpoint" ([crate::Usb] applies the effect before arming EP0). A disabled endpoint stays disabled.
    pub fn set_stalled(&self, address: EndpointAddress, stalled: bool) {
        let n = address.number() as usize;
        let layout = Self::layout();
        if n == 0 || n >= layout.count as usize || !layout.dir(n, address.direction()).is_used() {
            return;
        }
        let r = &self.regs;
        let st = self.state;
        match address.direction() {
            Direction::In => {
                let c = offset::diepctl(n);
                critical_section::with(|_| {
                    if r.read(c) & ctl::USBAEP == 0 {
                        return;
                    }
                    if stalled {
                        stop_in(r, n, true);
                        set_ctl(r, c, |v| v | ctl::STALL, 0);
                    } else {
                        set_ctl(r, c, |v| v & !ctl::STALL, ctl::SD0PID);
                    }
                });
                st.eps[n].in_waker.wake();
            }
            Direction::Out => {
                let c = offset::doepctl(n);
                critical_section::with(|cs| {
                    if r.read(c) & ctl::USBAEP == 0 {
                        return;
                    }
                    if stalled {
                        set_ctl(r, c, |v| v | ctl::STALL, 0);
                    } else {
                        set_ctl(r, c, |v| v & !ctl::STALL, ctl::SD0PID);
                        arm_out(r, layout, st, n, cs);
                    }
                });
                st.eps[n].out_waker.wake();
            }
        }
    }

    /// Whether a class endpoint is halted.
    pub fn is_stalled(&self, address: EndpointAddress) -> bool {
        let n = address.number() as usize;
        if n == 0 || n >= Self::layout().count as usize {
            return false;
        }
        let c = match address.direction() {
            Direction::In => offset::diepctl(n),
            Direction::Out => offset::doepctl(n),
        };
        self.regs.read(c) & ctl::STALL != 0
    }
}

impl<D: OtgDescriptors> Driver<D, super::regs::Mmio> {
    /// Makes this driver the one [super::on_interrupt] serves. Call it before unmasking the interrupt.
    pub fn register(&self) {
        super::register(super::Active {
            regs: self.regs,
            layout: Self::layout(),
            state: self.state,
        })
    }
}

impl<D: OtgDescriptors, R: Regs + Copy> crate::Backend for Driver<D, R> {
    type Ep0 = Ep0<R>;
    const ADDRESS_BEFORE_STATUS: bool = true;
    fn ep0(&self) -> Ep0<R> {
        Driver::ep0(self)
    }
    fn bus(&self) -> &'static BusState {
        self.state.bus
    }
    fn set_address(&self, address: u8) {
        Driver::set_address(self, address)
    }
    fn set_enabled(&self, endpoint: EndpointAddress, enabled: bool) {
        Driver::set_enabled(self, endpoint, enabled)
    }
    fn set_stalled(&self, endpoint: EndpointAddress, stalled: bool) {
        Driver::set_stalled(self, endpoint, stalled)
    }
    /// DSTS.SUSPSTS: "set as long as a suspend condition is detected on the USB".
    fn is_suspended(&self) -> bool {
        self.regs.read(offset::DSTS) & dsts::SUSPSTS != 0
    }
    fn speed(&self) -> Speed {
        Driver::speed(self)
    }
}

/// What a bus reset leaves, §62.15.6 "Endpoint initialization on USB reset": "1. Set the NAK bit for all OUT
/// endpoints ... 2. Unmask the following interrupt bits – INEP0 = 1 in OTG_DAINTMSK ... – OUTEP0 = 1 ... 3. Set up
/// the data FIFO RAM for each of the FIFOs ... 4. Program the following fields in the endpoint-specific registers
/// for control OUT endpoint 0 to receive a SETUP packet – STUPCNT = 3 in OTG_DOEPTSIZ0". Before that, every
/// transfer is stopped ("Transfer Stop Programming": the armed OUT endpoints disabled under the global OUT NAK,
/// [stop_out]; the IN endpoints by [stop_in]) and the FIFOs flushed: a packet from before the reset is no one's
/// any more. The address goes back to 0 and the speed to full until enumeration is done.
pub(crate) fn bus_reset<R: Regs>(r: &R, layout: &Layout, st: StateRef) {
    critical_section::with(|cs| {
        stop_out(r, layout, st, !0);
        for n in 0..layout.count as usize {
            stop_in(r, n, false);
            set_ctl(r, offset::doepctl(n), |v| v & !ctl::STALL, ctl::SNAK);
            r.write(offset::diepint(n), !0);
            r.write(offset::doepint(n), !0);
            st.eps[n].set_out(cs, OutQueue::DISABLED);
        }
        flush_tx(r, grstctl::TXFNUM_ALL);
        flush_rx(r);
        r.write(offset::DAINTMSK, daint::in_ep(0) | daint::out_ep(0));
        let v = r.read(offset::DCFG) & !dcfg::DAD;
        r.write(offset::DCFG, v);
        r.write(offset::GRXFSIZ, layout.rx_words as u32);
        for n in 0..layout.count as usize {
            let d = &layout.eps[n].inp;
            if d.is_used() {
                r.write(offset::dieptxf(n), fsiz::value(d.start, d.words));
            }
        }
        r.write(
            offset::diepctl(0),
            ctl::mpsiz0(layout.eps[0].inp.max_packet[0] as u8) | ctl::SNAK,
        );
        r.write(offset::doeptsiz(0), 3 << tsiz::STUPCNT_SHIFT);
        st.speed.borrow(cs).set(Speed::Full);
        st.ep0.in_done.borrow(cs).set(false);
        st.ep0.out_done.borrow(cs).set(None);
        st.bus.reset(cs);
    });
    for ep in st.eps {
        ep.in_waker.wake();
        ep.out_waker.wake();
    }
    st.bus.waker.wake();
}

/// Pops `bcnt` bytes of a packet off the receive FIFO into the OUT buffers from word `start`, `room` words there
/// at most; the rest is popped and dropped. §62.15.6 "Packet read": "If the received packet's byte count is not 0,
/// the byte count amount of data is popped from the receive data FIFO and stored in memory".
fn pop_into<R: Regs>(r: &R, st: StateRef, start: usize, room: usize, bcnt: usize) {
    for i in 0..bcnt.div_ceil(4) {
        let w = r.pop();
        if i < room {
            // Safety: the caller owns these words: EP0's buffer, or the one the endpoint's queue fills
            unsafe { st.set_word(start + i, w) }
        }
    }
}

/// The interrupt handler of a device with `layout` on `r`.
///
/// - USBRST: the bus reset here ([bus_reset]), so EP0 can take a SETUP whatever the executor does. ENUMDNE: the
///   speed from DSTS.ENUMSPD and EP0's max packet size (§62.15.6 "Endpoint initialization on enumeration
///   completion": "read the OTG_DSTS register to determine the enumeration speed ... Program the MPSIZ field in
///   OTG_DIEPCTL0").
/// - RXFLVL: every entry of the receive FIFO popped (§62.15.6 "Packet read"): a SETUP's two words kept for EP0, an
///   OUT packet copied into its endpoint's buffer (dropped when the endpoint has none armed); the transfer-done
///   and setup-done entries carry no data, their interrupts follow.
/// - OEPINT: STUP raises [event::SETUP]; XFRC completes the packet in the armed buffer, and a double-buffered
///   endpoint is armed again on its free buffer. IEPINT: XFRC wakes the endpoint's task.
/// - USBSUSP and WKUPINT: [event::SUSPEND], [event::RESUME].
///
/// Each flag is cleared with a write of 1. Returns at once when nothing it unmasked is pending.
pub fn interrupt<R: Regs>(r: &R, layout: &Layout, st: StateRef) {
    let flags = r.read(offset::GINTSTS) & r.read(offset::GINTMSK);
    if flags == 0 {
        return;
    }
    let mut events = 0;
    if flags & gint::USBRST != 0 {
        // a suspend from before the reset is over: the reset ended it (embassy-usb-synopsys-otg: a USBSUSP left
        // set makes the next look report a suspend that did not happen)
        r.write(offset::GINTSTS, gint::USBRST | gint::USBSUSP);
        bus_reset(r, layout, st);
    }
    if flags & gint::ENUMDNE != 0 {
        r.write(offset::GINTSTS, gint::ENUMDNE | gint::USBSUSP);
        let spd = (r.read(offset::DSTS) & dsts::ENUMSPD) >> dsts::ENUMSPD_SHIFT;
        let speed = if spd == dsts::ENUMSPD_HIGH {
            Speed::High
        } else {
            Speed::Full
        };
        critical_section::with(|cs| st.speed.borrow(cs).set(speed));
        set_ctl(
            r,
            offset::diepctl(0),
            |v| (v & !ctl::MPSIZ) | ctl::mpsiz0(layout.eps[0].inp.max_packet[0] as u8),
            0,
        );
        st.bus.waker.wake();
    }
    if flags & gint::RXFLVL != 0 {
        receive(r, layout, st);
    }
    if flags & gint::OEPINT != 0 {
        let all = r.read(offset::DAINT) & r.read(offset::DAINTMSK);
        for n in 0..layout.count as usize {
            if all & daint::out_ep(n) == 0 {
                continue;
            }
            let ints = r.read(offset::doepint(n)) & DOEPMSK;
            r.write(offset::doepint(n), ints);
            critical_section::with(|cs| {
                if n == 0 {
                    if ints & epint::XFRC != 0 {
                        let len = st.ep0.out_done.borrow(cs).get().unwrap_or(0);
                        st.ep0.out_done.borrow(cs).set(Some(len | DONE));
                    }
                    if ints & epint::STUP != 0 {
                        st.bus.raise(cs, event::SETUP);
                    }
                } else if ints & epint::XFRC != 0 {
                    let ep = &st.eps[n];
                    let mut q = ep.out(cs);
                    if q.completed() {
                        ep.set_out(cs, q);
                        arm_out(r, layout, st, n, cs);
                    }
                }
            });
            st.eps[n].out_waker.wake();
            if n == 0 {
                st.bus.waker.wake();
            }
        }
    }
    if flags & gint::IEPINT != 0 {
        let all = r.read(offset::DAINT) & r.read(offset::DAINTMSK);
        for n in 0..layout.count as usize {
            if all & daint::in_ep(n) == 0 {
                continue;
            }
            let ints = r.read(offset::diepint(n)) & DIEPMSK;
            r.write(offset::diepint(n), ints);
            if n == 0 && ints & epint::XFRC != 0 {
                critical_section::with(|cs| st.ep0.in_done.borrow(cs).set(true));
            }
            st.eps[n].in_waker.wake();
            if n == 0 {
                st.bus.waker.wake();
            }
        }
    }
    if flags & gint::USBSUSP != 0 {
        r.write(offset::GINTSTS, gint::USBSUSP);
        events |= event::SUSPEND;
    }
    if flags & gint::WKUPINT != 0 {
        r.write(offset::GINTSTS, gint::WKUPINT);
        events |= event::RESUME;
    }
    if events != 0 {
        critical_section::with(|cs| st.bus.raise(cs, events));
        st.bus.waker.wake();
    }
}

/// Marks EP0's OUT length as completed by XFRC (the length itself is at most 64).
pub(crate) const DONE: u16 = 0x8000;

/// Empties the receive FIFO (§62.15.6 "Packet read" steps 1-6: "Reading an empty receive FIFO can result in
/// undefined core behavior", so RXFLVL is read before every pop).
fn receive<R: Regs>(r: &R, layout: &Layout, st: StateRef) {
    while r.read(offset::GINTSTS) & gint::RXFLVL != 0 {
        let s = r.read(offset::GRXSTSP);
        let n = (s & grxsts::EPNUM) as usize;
        let bcnt = ((s & grxsts::BCNT) >> grxsts::BCNT_SHIFT) as usize;
        match (s & grxsts::PKTSTS) >> grxsts::PKTSTS_SHIFT {
            grxsts::SETUP_DATA => {
                // "3 words per SETUP packet are required to store 8 bytes of SETUP data and 4 bytes of SETUP
                // status": the status was the pop above, the 8 bytes are two words
                let w = [r.pop(), r.pop()];
                for _ in 2..bcnt.div_ceil(4) {
                    r.pop();
                }
                if bcnt == 8 {
                    critical_section::with(|cs| st.ep0.setup.borrow(cs).set(w));
                }
            }
            grxsts::OUT_DATA if n == 0 => {
                let d = &layout.eps[0].out;
                pop_into(r, st, d.start as usize, d.words as usize, bcnt);
                let len = bcnt.min(4 * d.words as usize) as u16;
                critical_section::with(|cs| st.ep0.out_done.borrow(cs).set(Some(len)));
            }
            grxsts::OUT_DATA if n < layout.count as usize => {
                let d = &layout.eps[n].out;
                let ep = &st.eps[n];
                let target = critical_section::with(|cs| ep.out(cs).filling());
                match target {
                    Some(b) => {
                        let start = d.start as usize + b * d.words as usize;
                        pop_into(r, st, start, d.words as usize, bcnt);
                        let len = bcnt.min(4 * d.words as usize) as u16;
                        critical_section::with(|cs| {
                            let mut q = ep.out(cs);
                            q.copied(len);
                            ep.set_out(cs, q);
                        });
                    }
                    None => pop_into(r, st, 0, 0, bcnt),
                }
            }
            grxsts::OUT_DATA => pop_into(r, st, 0, 0, bcnt),
            // GLOBAL_OUT_NAK, OUT_DONE, SETUP_DONE: no data; OUT_DONE and SETUP_DONE raise XFRC and STUP
            _ => {}
        }
    }
}
