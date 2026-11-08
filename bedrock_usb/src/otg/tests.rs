//! [Usb::run](crate::Usb::run) on the simulated OTG core, driven as a host would at full and at high speed:
//! enumeration with the address before the status stage, the configuration of each speed, the class endpoints,
//! halts and toggles, resets and a SETUP that cuts a transfer short. Every test ends with the core's rule
//! violations checked empty (see `sim`).

extern crate std;

use core::future::Future;
use core::pin::Pin;
use core::task::{Context, Waker};
use std::boxed::Box;
use std::cell::RefCell;
use std::rc::Rc;
use std::vec::Vec;

use super::regs::{ctl, daint, fsiz, offset, tsiz};
use super::sim::{Handshake, Sim};
use super::{Core, Driver, Phy, State};
use crate::buffer::Double;
use crate::control::{Handler, OutResponse};
use crate::desc::{Device, Endpoint, Function, Interface, Item, Str};
use crate::{Buffering, Setup, Speed, Usb, descriptor_type};

// interface 0: the loopback pair; interface 1: nothing in setting 0, a bulk IN in setting 1; interface 2: an
// interrupt IN
const DEVICE: Device = Device {
    high_speed: true,
    serial: Str::Runtime,
    functions: &[
        Function {
            interfaces: &[Interface {
                items: &[
                    Item::Endpoint(Endpoint::bulk_out(1, 64, Buffering::Double)),
                    Item::Endpoint(Endpoint::bulk_in(2, 64, Buffering::Double)),
                ],
                ..Interface::vendor()
            }],
            ..Function::vendor()
        },
        Function {
            interfaces: &[
                Interface::vendor(),
                Interface {
                    alt_setting: 1,
                    items: &[Item::Endpoint(Endpoint::bulk_in(3, 64, Buffering::Single))],
                    ..Interface::vendor()
                },
            ],
            ..Function::vendor()
        },
        Function {
            interfaces: &[Interface {
                items: &[Item::Endpoint(Endpoint::interrupt_in(4, 16, 1))],
                ..Interface::vendor()
            }],
            ..Function::vendor()
        },
    ],
    ..Device::new(0xc0de, 0xcafe)
};
crate::descriptors!(Desc = DEVICE, otg = Core::H7_OTG_HS);

#[derive(Clone, Debug, PartialEq, Eq)]
enum Seen {
    Reset,
    Configured(bool),
    Suspended(bool),
    Alt(u8, u8),
    Out(u8, Vec<u8>),
}

#[derive(Clone, Default)]
struct Log(Rc<RefCell<Vec<Seen>>>);

impl Log {
    fn take(&self) -> Vec<Seen> {
        core::mem::take(&mut self.0.borrow_mut())
    }
}

impl Handler for Log {
    fn reset(&mut self) {
        self.0.borrow_mut().push(Seen::Reset)
    }
    fn configured(&mut self, configured: bool) {
        self.0.borrow_mut().push(Seen::Configured(configured))
    }
    fn suspended(&mut self, suspended: bool) {
        self.0.borrow_mut().push(Seen::Suspended(suspended))
    }
    fn set_alternate_setting(&mut self, interface: u8, alt: u8) {
        self.0.borrow_mut().push(Seen::Alt(interface, alt))
    }
    fn control_out(&mut self, setup: &Setup, data: &[u8]) -> OutResponse {
        if setup.request_type == 0x41 {
            self.0
                .borrow_mut()
                .push(Seen::Out(setup.request, data.to_vec()));
            OutResponse::Accept
        } else {
            OutResponse::Unhandled
        }
    }
    fn string(&self, index: u8) -> Option<&str> {
        (index == crate::desc::STRING_SERIAL).then_some("0123456789AB")
    }
}

type St = State<{ Desc::OTG.count as usize }, { Desc::OTG.out_words as usize }>;
type Drv = Driver<Desc, &'static Sim>;

struct Bench {
    sim: &'static Sim,
    state: &'static St,
    run: Pin<Box<dyn Future<Output = ()>>>,
    log: Log,
    /// A second handle on the core for the test's endpoints and the interrupt handler.
    driver: &'static Drv,
    speed: Speed,
}

impl Drop for Bench {
    fn drop(&mut self) {
        if !std::thread::panicking() {
            assert_eq!(self.sim.take_errors(), Vec::<std::string::String>::new());
        }
    }
}

const SET_CONFIG_1: [u8; 8] = [0, 9, 1, 0, 0, 0, 0, 0];

impl Bench {
    fn new(speed: Speed) -> Self {
        let sim: &'static Sim = Box::leak(Box::new(Sim::new()));
        let state: &'static St = Box::leak(Box::new(State::new()));
        let driver = Driver::new(sim, state);
        driver.init(Phy::Ulpi {
            high_speed: true,
            xcvrdly: false,
        });
        assert!(!sim.connected(), "off the bus until connect");
        driver.connect();
        assert!(sim.connected());
        let side: &'static Drv = Box::leak(Box::new(Driver::new(sim, state)));
        let log = Log::default();
        let usb: &'static mut Usb<Desc, Drv, Log, 128> =
            Box::leak(Box::new(Usb::new(driver, log.clone())));
        let run = Box::pin(async move { usb.run().await });
        let mut b = Bench {
            sim,
            state,
            run,
            log,
            driver: side,
            speed,
        };
        b.reset();
        b
    }

    /// Interrupts and the run task until both are quiet.
    fn pump(&mut self) {
        for _ in 0..20 {
            let mut k = 0;
            while self.sim.irq() {
                self.driver.on_interrupt();
                k += 1;
                assert!(k < 100, "an interrupt flag the handler doesn't clear");
            }
            let _ = self
                .run
                .as_mut()
                .poll(&mut Context::from_waker(Waker::noop()));
        }
    }

    fn reset(&mut self) {
        self.sim.bus_reset();
        self.sim.enum_done(self.speed);
        self.pump();
    }

    /// An IN token, again while the device NAKs (the run task arms the endpoint between tries).
    fn host_in(&mut self, n: usize) -> Result<Vec<u8>, Handshake> {
        for _ in 0..8 {
            let r = self.sim.host_in(n);
            self.pump();
            if r != Err(Handshake::Nak) {
                return r;
            }
        }
        Err(Handshake::Nak)
    }

    fn host_out(&mut self, n: usize, data: &[u8]) -> Handshake {
        for _ in 0..8 {
            let r = self.sim.host_out(n, data);
            self.pump();
            if r != Handshake::Nak {
                return r;
            }
        }
        Handshake::Nak
    }

    fn setup(&mut self, setup: [u8; 8]) {
        assert_eq!(self.sim.host_setup(setup), Handshake::Ack);
        self.pump();
    }

    /// A control read: the data, or the handshake that ended it.
    fn control_in(&mut self, setup: [u8; 8]) -> Result<Vec<u8>, Handshake> {
        self.setup(setup);
        let length = u16::from_le_bytes([setup[6], setup[7]]) as usize;
        let mut data = Vec::new();
        loop {
            let p = self.host_in(0)?;
            let short = p.len() < 64;
            data.extend(p);
            if short || data.len() >= length {
                break;
            }
        }
        assert_eq!(self.host_out(0, &[]), Handshake::Ack, "status OUT");
        Ok(data)
    }

    /// A request without data stage: Ack when its status stage came.
    fn control_none(&mut self, setup: [u8; 8]) -> Handshake {
        self.setup(setup);
        match self.host_in(0) {
            Ok(p) => {
                assert!(p.is_empty(), "status IN is zero-length");
                Handshake::Ack
            }
            Err(h) => h,
        }
    }

    /// A control write with `data` in 64-byte packets.
    fn control_out(&mut self, setup: [u8; 8], data: &[u8]) -> Handshake {
        self.setup(setup);
        for chunk in data.chunks(64) {
            match self.host_out(0, chunk) {
                Handshake::Ack => {}
                h => return h,
            }
        }
        match self.host_in(0) {
            Ok(p) => {
                assert!(p.is_empty());
                Handshake::Ack
            }
            Err(h) => h,
        }
    }

    /// The configuration descriptor of the speed the bench runs at.
    fn config(&self) -> &'static [u8] {
        match self.speed {
            Speed::Full => &Desc::CONFIG_DESC,
            Speed::High => &Desc::CONFIG_DESC_HS,
        }
    }

    fn enumerate(&mut self) {
        let d = self.control_in([0x80, 6, 0, 1, 0, 0, 64, 0]).unwrap();
        assert_eq!(d, Desc::DEVICE_DESC);
        self.reset();
        // SET_ADDRESS: DCFG.DAD before the status stage (§62.15.6), the core answers on 0 until it is over
        self.setup([0, 5, 9, 0, 0, 0, 0, 0]);
        assert_eq!(self.host_in(0), Ok(Vec::new()));
        assert_eq!(self.sim.address(), 9);
        let c = self.control_in([0x80, 6, 0, 2, 0, 0, 255, 0]).unwrap();
        assert_eq!(c, self.config());
        assert_eq!(self.control_none(SET_CONFIG_1), Handshake::Ack);
        for n in 1..=4 {
            self.sim.host_reset_toggles(n);
        }
    }

    fn doepctl(&self, n: usize) -> u32 {
        self.sim.reg(offset::doepctl(n))
    }

    fn diepctl(&self, n: usize) -> u32 {
        self.sim.reg(offset::diepctl(n))
    }
}

/// The address goes to DCFG once SET_ADDRESS is in, before its status IN.
#[test]
fn address_before_the_status_stage() {
    let mut b = Bench::new(Speed::Full);
    b.setup([0, 5, 9, 0, 0, 0, 0, 0]);
    // the setup-done entry comes at the host's first IN token, which the core NAKs; then the run task acts
    assert_eq!(b.sim.host_in(0), Err(Handshake::Nak));
    b.pump();
    assert_eq!(b.sim.address(), 9, "before the status stage");
    assert_eq!(b.sim.host_in(0), Ok(Vec::new()));
    b.pump();
    assert_eq!(b.sim.address(), 9);
}

fn enumeration_at(speed: Speed) {
    let mut b = Bench::new(speed);
    assert_eq!(b.log.take(), [Seen::Reset]);
    b.enumerate();
    assert_eq!(b.driver.speed(), speed);
    assert_eq!(b.log.take(), [Seen::Reset, Seen::Configured(true)]);
    let s = b.state.status();
    assert!(s.configured && !s.suspended);
    let (bulk, other) = match speed {
        Speed::Full => (64, &Desc::CONFIG_DESC_HS[..]),
        Speed::High => (512, &Desc::CONFIG_DESC[..]),
    };
    // OUT armed for one packet of the speed's size, IN active with NAK until there is data
    let o = b.doepctl(1);
    assert_eq!(o & ctl::MPSIZ, bulk);
    assert!(o & ctl::USBAEP != 0 && o & ctl::EPENA != 0 && o & ctl::NAKSTS == 0);
    assert_eq!(o & ctl::EPTYP, ctl::EPTYP_BULK);
    assert_eq!(b.sim.reg(offset::doeptsiz(1)), tsiz::one(bulk as usize));
    let i = b.diepctl(2);
    assert_eq!(i & ctl::MPSIZ, bulk);
    assert!(i & ctl::USBAEP != 0 && i & ctl::NAKSTS != 0 && i & ctl::EPENA == 0);
    assert_eq!(
        (i & ctl::TXFNUM) >> ctl::TXFNUM_SHIFT,
        2,
        "FIFO n for endpoint n"
    );
    let int = b.diepctl(4);
    assert_eq!(
        (int & ctl::MPSIZ, int & ctl::EPTYP),
        (16, ctl::EPTYP_INTERRUPT)
    );
    assert_eq!(b.diepctl(3) & ctl::USBAEP, 0, "interface 1 is in setting 0");
    assert_eq!(
        b.sim.reg(offset::DAINTMSK),
        daint::in_ep(0) | daint::out_ep(0) | daint::out_ep(1) | daint::in_ep(2) | daint::in_ep(4)
    );
    // the qualifier and the other speed's configuration, type patched
    assert_eq!(
        b.control_in([0x80, 6, 0, descriptor_type::DEVICE_QUALIFIER, 0, 0, 10, 0])
            .unwrap(),
        Desc::DEVICE_QUALIFIER
    );
    let mut o = other.to_vec();
    o[1] = descriptor_type::OTHER_SPEED_CONFIGURATION;
    assert_eq!(
        b.control_in([
            0x80,
            6,
            0,
            descriptor_type::OTHER_SPEED_CONFIGURATION,
            0,
            0,
            255,
            0
        ])
        .unwrap(),
        o
    );
    assert_eq!(b.control_in([0x80, 8, 0, 0, 0, 0, 1, 0]).unwrap(), [1]);
}

#[test]
fn enumeration_at_full_speed() {
    enumeration_at(Speed::Full);
}

#[test]
fn enumeration_at_high_speed() {
    enumeration_at(Speed::High);
}

/// The FIFO registers a bus reset leaves: the layout's, the receive FIFO first, the transmit FIFOs after it
/// without a gap or an overlap, all in the core's RAM; EP0 waiting for three SETUPs.
#[test]
fn fifo_registers_after_a_reset() {
    let b = Bench::new(Speed::High);
    let l = &Desc::OTG;
    assert_eq!(b.sim.reg(offset::GRXFSIZ), l.rx_words as u32);
    let mut next = l.rx_words;
    for n in 0..l.count as usize {
        let d = &l.eps[n].inp;
        if !d.is_used() {
            assert_eq!(b.sim.reg(offset::dieptxf(n)), 0);
            continue;
        }
        assert_eq!(d.start, next, "FIFO {n} follows the one before");
        assert_eq!(b.sim.reg(offset::dieptxf(n)), fsiz::value(d.start, d.words));
        assert!(
            d.words >= 16 && d.words as usize * 4 >= d.max_packet[1] as usize * d.packets as usize
        );
        next += d.words;
    }
    assert_eq!(next, l.fifo_end);
    assert!(l.fifo_end <= Core::H7_OTG_HS.fifo_words);
    // the double-buffered bulk IN holds two 512-byte packets, the single one one
    assert_eq!(l.eps[2].inp.words, 256);
    assert_eq!(l.eps[3].inp.words, 128);
    assert_eq!(
        b.sim.reg(offset::doeptsiz(0)) & tsiz::STUPCNT,
        3 << tsiz::STUPCNT_SHIFT
    );
    assert_eq!(
        b.sim.reg(offset::DAINTMSK),
        daint::in_ep(0) | daint::out_ep(0)
    );
    assert_eq!(b.sim.address(), 0);
}

fn echo(b: &Bench) -> Pin<Box<dyn Future<Output = ()>>> {
    let mut out = b.driver.bulk_out::<1, Double>();
    let mut inp = b.driver.bulk_in::<2, Double>();
    Box::pin(async move {
        let mut buf = [0u8; 512];
        loop {
            match out.read(&mut buf).await {
                Ok(n) => {
                    if inp.write(&buf[..n]).await.is_err() {
                        inp.wait_enabled().await;
                    }
                }
                Err(_) => out.wait_enabled().await,
            }
        }
    })
}

fn poll(f: &mut Pin<Box<dyn Future<Output = ()>>>) {
    let _ = f.as_mut().poll(&mut Context::from_waker(Waker::noop()));
}

fn loopback_at(speed: Speed) {
    let mut b = Bench::new(speed);
    b.enumerate();
    let mut e = echo(&b);
    let mps = match speed {
        Speed::Full => 64,
        Speed::High => 512,
    };
    for k in 0..20u8 {
        // full packets, a short one, a zero-length one
        let len = match k % 4 {
            3 => 0,
            2 => 13,
            _ => mps,
        };
        let p: Vec<u8> = (0..len).map(|i| i as u8 ^ k).collect();
        assert_eq!(b.sim.host_out(1, &p), Handshake::Ack);
        b.pump();
        poll(&mut e);
        b.pump();
        assert_eq!(b.host_in(2).unwrap(), p, "packet {k}");
    }
}

#[test]
fn loopback_at_full_speed() {
    loopback_at(Speed::Full);
}

#[test]
fn loopback_at_high_speed() {
    loopback_at(Speed::High);
}

/// `Double` on OUT: the core takes a second packet while the firmware hasn't read the first, and NAKs a third.
#[test]
fn double_buffered_out_takes_the_second_packet() {
    let mut b = Bench::new(Speed::High);
    b.enumerate();
    let p = |k: u8| -> Vec<u8> { (0..512).map(|i| (i as u8).wrapping_add(k)).collect() };
    assert_eq!(b.sim.host_out(1, &p(1)), Handshake::Ack);
    b.pump();
    assert_eq!(b.sim.host_out(1, &p(2)), Handshake::Ack, "second buffer");
    b.pump();
    assert_eq!(b.sim.host_out(1, &p(3)), Handshake::Nak, "both full");
    b.pump();
    let mut e = echo(&b);
    poll(&mut e);
    b.pump();
    assert_eq!(b.host_in(2).unwrap(), p(1));
    poll(&mut e);
    b.pump();
    assert_eq!(
        b.host_out(1, &p(3)),
        Handshake::Ack,
        "a buffer is free again"
    );
    poll(&mut e);
    b.pump();
    assert_eq!(b.host_in(2).unwrap(), p(2));
    poll(&mut e);
    b.pump();
    assert_eq!(b.host_in(2).unwrap(), p(3));
}

#[test]
fn halt_and_clear_reset_the_toggle() {
    let mut b = Bench::new(Speed::Full);
    b.enumerate();
    let mut e = echo(&b);
    // one packet each way: both toggles at DATA1
    assert_eq!(b.sim.host_out(1, &[1, 2, 3]), Handshake::Ack);
    b.pump();
    poll(&mut e);
    b.pump();
    assert_eq!(b.host_in(2).unwrap(), [1, 2, 3]);
    assert!(b.doepctl(1) & ctl::DPID != 0 && b.diepctl(2) & ctl::DPID != 0);
    // SET_FEATURE(ENDPOINT_HALT) on EP2 IN and EP1 OUT
    assert_eq!(b.control_none([2, 3, 0, 0, 0x82, 0, 0, 0]), Handshake::Ack);
    assert_eq!(b.control_none([2, 3, 0, 0, 0x01, 0, 0, 0]), Handshake::Ack);
    assert_eq!(b.sim.host_in(2), Err(Handshake::Stall));
    assert_eq!(b.sim.host_out(1, &[9]), Handshake::Stall);
    assert_eq!(
        b.control_in([0x82, 0, 0, 0, 0x82, 0, 2, 0]).unwrap(),
        [1, 0]
    );
    // CLEAR_FEATURE: DATA0 both ways, the host starts over too
    assert_eq!(b.control_none([2, 1, 0, 0, 0x82, 0, 0, 0]), Handshake::Ack);
    assert_eq!(b.control_none([2, 1, 0, 0, 0x01, 0, 0, 0]), Handshake::Ack);
    b.sim.host_reset_toggles(1);
    b.sim.host_reset_toggles(2);
    assert_eq!(b.doepctl(1) & (ctl::DPID | ctl::STALL), 0);
    assert_eq!(b.diepctl(2) & (ctl::DPID | ctl::STALL), 0);
    assert_eq!(b.sim.host_in(2), Err(Handshake::Nak));
    assert_eq!(
        b.control_in([0x82, 0, 0, 0, 0x82, 0, 2, 0]).unwrap(),
        [0, 0]
    );
    assert_eq!(b.host_out(1, &[4, 5]), Handshake::Ack);
    poll(&mut e);
    b.pump();
    assert_eq!(b.host_in(2).unwrap(), [4, 5]);
}

#[test]
fn deconfigure_and_reconfigure() {
    let mut b = Bench::new(Speed::High);
    b.enumerate();
    b.log.take();
    assert_eq!(b.control_none([0, 9, 0, 0, 0, 0, 0, 0]), Handshake::Ack);
    assert!(!b.state.status().configured);
    assert_eq!(b.doepctl(1) & ctl::USBAEP, 0);
    assert_eq!(b.diepctl(2) & ctl::USBAEP, 0);
    assert_eq!(b.sim.host_out(1, &[1]), Handshake::None, "not active");
    assert_eq!(b.sim.host_in(2), Err(Handshake::None));
    assert_eq!(b.control_none(SET_CONFIG_1), Handshake::Ack);
    b.sim.host_reset_toggles(1);
    b.sim.host_reset_toggles(2);
    assert_eq!(
        b.log.take(),
        [Seen::Configured(false), Seen::Configured(true)]
    );
    let mut e = echo(&b);
    assert_eq!(b.host_out(1, &[7; 512]), Handshake::Ack);
    poll(&mut e);
    b.pump();
    assert_eq!(b.host_in(2).unwrap(), [7; 512]);
}

#[test]
fn set_interface_enables_its_endpoints_only() {
    let mut b = Bench::new(Speed::Full);
    b.enumerate();
    b.log.take();
    assert_eq!(b.control_none([1, 11, 1, 0, 1, 0, 0, 0]), Handshake::Ack);
    assert_eq!(b.log.take(), [Seen::Alt(1, 1)]);
    assert!(b.diepctl(3) & ctl::USBAEP != 0, "setting 1 has the IN");
    assert!(b.doepctl(1) & ctl::EPENA != 0, "interface 0 untouched");
    assert_eq!(b.control_none([1, 11, 0, 0, 1, 0, 0, 0]), Handshake::Ack);
    assert_eq!(b.diepctl(3) & ctl::USBAEP, 0);
    assert_eq!(b.control_none([1, 11, 2, 0, 1, 0, 0, 0]), Handshake::Stall);
}

/// A bus reset while an IN packet waits in its FIFO and an OUT packet waits unread: both are dropped, the
/// endpoints' tasks hear `Disabled`, and after the next enumeration the loopback works from DATA0.
#[test]
fn a_reset_during_a_transfer() {
    let mut b = Bench::new(Speed::High);
    b.enumerate();
    let mut inp = b.driver.bulk_in::<2, Double>();
    let mut out = b.driver.bulk_out::<1, Double>();
    let mut w = Box::pin(async move {
        inp.write(&[1; 100]).await.unwrap();
        // the reset comes while this one waits for the first to go
        let r = inp.write(&[2; 100]).await;
        assert_eq!(r, Err(super::EndpointError::Disabled));
        let mut buf = [0u8; 512];
        assert_eq!(
            out.read(&mut buf).await,
            Err(super::EndpointError::Disabled)
        );
        // configured again: a loopback from DATA0
        out.wait_enabled().await;
        let n = out.read(&mut buf).await.unwrap();
        inp.write(&buf[..n]).await.unwrap();
    });
    let mut cx = Context::from_waker(Waker::noop());
    assert_eq!(b.sim.host_out(1, &[5; 5]), Handshake::Ack);
    b.pump();
    assert!(w.as_mut().poll(&mut cx).is_pending());
    assert!(b.diepctl(2) & ctl::EPENA != 0, "the first packet is queued");
    assert_eq!(b.sim.with(|i| i.tx[2].len()), 25);
    b.reset();
    assert_eq!(b.diepctl(2) & (ctl::EPENA | ctl::USBAEP), 0);
    assert_eq!(b.sim.with(|i| i.tx[2].len()), 0, "FIFO flushed");
    assert!(w.as_mut().poll(&mut cx).is_pending());
    b.enumerate();
    assert_eq!(b.host_out(1, &[3; 30]), Handshake::Ack);
    assert!(w.as_mut().poll(&mut cx).is_ready());
    b.pump();
    assert_eq!(b.host_in(2).unwrap(), [3; 30]);
}

/// A host that gives up on a control read after the SETUP and sends the next SETUP: the data packet queued on EP0
/// IN for the first one is stopped and flushed (its EPENA would keep the next one out), and the second request is
/// answered with its own data.
#[test]
fn a_stale_ep0_in_after_a_new_setup() {
    let mut b = Bench::new(Speed::Full);
    b.setup([0x80, 6, 0, 2, 0, 0, 255, 0]);
    assert_eq!(
        b.sim.host_out(0, &[]),
        Handshake::Nak,
        "setup-done, not armed yet"
    );
    b.pump();
    assert!(
        b.diepctl(0) & ctl::EPENA != 0,
        "the configuration is queued"
    );
    // the host gives up and asks for the device descriptor
    assert_eq!(
        b.control_in([0x80, 6, 0, 1, 0, 0, 18, 0]).unwrap(),
        Desc::DEVICE_DESC
    );
    assert_eq!(
        b.control_in([0x80, 6, 0, 2, 0, 0, 255, 0]).unwrap(),
        Desc::CONFIG_DESC
    );
}

#[test]
fn unknown_request_stalls_and_the_next_one_works() {
    let mut b = Bench::new(Speed::High);
    assert_eq!(
        b.control_in([0xc0, 0x55, 0, 0, 0, 0, 8, 0]),
        Err(Handshake::Stall)
    );
    assert_eq!(
        b.control_in([0x80, 6, 0, 1, 0, 0, 18, 0]).unwrap(),
        Desc::DEVICE_DESC
    );
}

#[test]
fn class_write_with_a_data_stage() {
    let mut b = Bench::new(Speed::Full);
    b.enumerate();
    b.log.take();
    let data: Vec<u8> = (0..100).collect();
    assert_eq!(
        b.control_out([0x41, 7, 0, 0, 0, 0, 100, 0], &data),
        Handshake::Ack
    );
    assert_eq!(b.log.take(), [Seen::Out(7, data)]);
}

#[test]
fn runtime_serial_string() {
    let mut b = Bench::new(Speed::High);
    let i = crate::desc::STRING_SERIAL;
    let s = b.control_in([0x80, 6, i, 3, 9, 4, 255, 0]).unwrap();
    assert_eq!(s.len(), 2 + 24);
    assert_eq!(&s[2..4], b"0\0");
}

#[test]
fn suspend_and_resume() {
    let mut b = Bench::new(Speed::High);
    b.enumerate();
    b.log.take();
    b.sim.suspend();
    b.pump();
    assert!(b.state.status().suspended);
    assert_eq!(b.log.take(), [Seen::Suspended(true)]);
    b.sim.resume();
    b.pump();
    let s = b.state.status();
    assert!(!s.suspended && s.configured);
    assert_eq!(b.log.take(), [Seen::Suspended(false)]);
}

#[test]
fn interrupt_in_report() {
    let mut b = Bench::new(Speed::High);
    b.enumerate();
    let mut int = b.driver.interrupt_in::<4>();
    let mut f = Box::pin(async move { int.write(&[0xaa; 16]).await.unwrap() });
    assert!(
        f.as_mut()
            .poll(&mut Context::from_waker(Waker::noop()))
            .is_ready()
    );
    assert_eq!(b.host_in(4).unwrap(), [0xaa; 16]);
    assert_eq!(b.sim.host_in(4), Err(Handshake::Nak));
}
