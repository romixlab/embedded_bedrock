//! [Usb::run] on the simulated peripheral, driven as a host would: whole control transfers, bus resets, suspend.

extern crate std;

use core::future::Future;
use core::pin::Pin;
use core::task::{Context, Poll, Waker};
use std::boxed::Box;
use std::cell::RefCell;
use std::rc::Rc;
use std::vec::Vec;

use super::Usb;
use crate::buffer::Double;
use crate::control::{Handler, OutResponse};
use crate::desc::{Device, Endpoint, Function, Interface, Item, Str};
use crate::v4::chep::{Stat, stat_rx, stat_tx};
use crate::v4::sim::{Handshake, Sim};
use crate::v4::{Driver, State, Status, interrupt};
use crate::{Buffering, Setup};

// interface 0: the loopback pair; interface 1: no endpoint in setting 0, a bulk IN in setting 1
const DEVICE: Device = Device {
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
    ],
    ..Device::new(0xc0de, 0xcafe)
};
crate::descriptors!(Desc = DEVICE);

/// What the handler saw, in order.
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

type St = State<{ Desc::LAYOUT.count as usize }>;
type Dev = Usb<Desc, Driver<Desc, &'static Sim>, Log>;

/// The device under test: the simulated chip, its state, the run future, the handler's log.
struct Bench {
    sim: &'static Sim,
    state: &'static St,
    run: Pin<Box<dyn Future<Output = ()>>>,
    log: Log,
    driver: &'static Driver<Desc, &'static Sim>,
}

impl Bench {
    fn new() -> Self {
        let sim: &'static Sim = Box::leak(Box::new(Sim::new()));
        let state: &'static St = Box::leak(Box::new(State::new()));
        let driver = Driver::new(sim, state);
        driver.init();
        // a second handle on the same chip for the test's own endpoint access
        let side: &'static Driver<Desc, &'static Sim> =
            Box::leak(Box::new(Driver::new(sim, state)));
        let log = Log::default();
        let usb: &'static mut Dev = Box::leak(Box::new(Usb::new(driver, log.clone())));
        let run = Box::pin(async move {
            usb.run().await;
        });
        let mut b = Bench {
            sim,
            state,
            run,
            log,
            driver: side,
        };
        b.reset();
        b
    }

    /// Interrupts and the run task until both are quiet.
    fn pump(&mut self) {
        for _ in 0..20 {
            while self.sim.irq() {
                interrupt(&self.sim, &Desc::LAYOUT, self.state.as_ref());
            }
            let _ = self
                .run
                .as_mut()
                .poll(&mut Context::from_waker(Waker::noop()));
        }
    }

    fn reset(&mut self) {
        self.sim.bus_reset();
        self.pump();
    }

    fn status(&self) -> Status {
        self.state.status()
    }

    /// A control read: the data, or the handshake that ended it.
    fn control_in(&mut self, setup: [u8; 8]) -> Result<Vec<u8>, Handshake> {
        assert_eq!(self.sim.host_setup(0, setup), Handshake::Ack);
        self.pump();
        let length = u16::from_le_bytes([setup[6], setup[7]]) as usize;
        let mut data = Vec::new();
        loop {
            let p = self.sim.host_in(0)?;
            self.pump();
            let short = p.len() < 64;
            data.extend(p);
            if short || data.len() >= length {
                break;
            }
        }
        assert_eq!(self.sim.host_out(0, &[]), Handshake::Ack, "status OUT");
        self.pump();
        Ok(data)
    }

    /// A request without data stage: Ack when its status stage came.
    fn control_none(&mut self, setup: [u8; 8]) -> Handshake {
        assert_eq!(self.sim.host_setup(0, setup), Handshake::Ack);
        self.pump();
        let r = match self.sim.host_in(0) {
            Ok(p) => {
                assert!(p.is_empty(), "status IN is zero-length");
                Handshake::Ack
            }
            Err(h) => h,
        };
        self.pump();
        r
    }

    /// A control write with `data` in 64-byte packets.
    fn control_out(&mut self, setup: [u8; 8], data: &[u8]) -> Handshake {
        assert_eq!(self.sim.host_setup(0, setup), Handshake::Ack);
        self.pump();
        for chunk in data.chunks(64) {
            match self.sim.host_out(0, chunk) {
                Handshake::Ack => {}
                h => return h,
            }
            self.pump();
        }
        let r = match self.sim.host_in(0) {
            Ok(p) => {
                assert!(p.is_empty());
                Handshake::Ack
            }
            Err(h) => h,
        };
        self.pump();
        r
    }

    fn enumerate(&mut self) {
        let d = self.control_in([0x80, 6, 0, 1, 0, 0, 64, 0]).unwrap();
        assert_eq!(d, Desc::DEVICE_DESC);
        self.reset();
        assert_eq!(self.control_none([0, 5, 9, 0, 0, 0, 0, 0]), Handshake::Ack);
        let c = self.control_in([0x80, 6, 0, 2, 0, 0, 255, 0]).unwrap();
        assert_eq!(c, Desc::CONFIG_DESC);
        assert_eq!(self.control_none(SET_CONFIG_1), Handshake::Ack);
    }
}

const SET_CONFIG_1: [u8; 8] = [0, 9, 1, 0, 0, 0, 0, 0];

fn stats(sim: &Sim, n: usize) -> (Stat, Stat) {
    let c = sim.chep_value(n);
    (stat_rx(c), stat_tx(c))
}

#[test]
fn enumeration_configures_the_endpoints() {
    let mut b = Bench::new();
    assert_eq!(b.log.take(), [Seen::Reset]);
    assert_eq!(stats(b.sim, 1).0, Stat::Disabled);
    b.enumerate();
    // SET_ADDRESS took effect after its status stage (R14)
    b.sim.with(|i| assert_eq!(i.daddr & 0x7f, 9));
    assert_eq!(b.log.take(), [Seen::Reset, Seen::Configured(true)]);
    let s = b.status();
    assert!(s.configured && s.was_configured && !s.suspended);
    assert_eq!((s.resets, s.resets_after_configured), (2, 0));
    // OUT VALID, IN NAK until there is data; interface 1 is in setting 0, its IN stays off
    assert_eq!(stats(b.sim, 1).0, Stat::Valid);
    assert_eq!(stats(b.sim, 2).1, Stat::Nak);
    assert_eq!(stats(b.sim, 3).1, Stat::Disabled);
    // GET_CONFIGURATION
    assert_eq!(b.control_in([0x80, 8, 0, 0, 0, 0, 1, 0]).unwrap(), [1]);
}

#[test]
fn runtime_serial_string() {
    let mut b = Bench::new();
    let i = crate::desc::STRING_SERIAL;
    let s = b.control_in([0x80, 6, i, 3, 9, 4, 255, 0]).unwrap();
    assert_eq!(s.len(), 2 + 24);
    assert_eq!(&s[2..4], b"0\0");
}

#[test]
fn reset_after_configured_is_counted() {
    let mut b = Bench::new();
    b.enumerate();
    b.log.take();
    b.reset();
    let s = b.status();
    assert!(!s.configured && s.was_configured);
    assert_eq!(s.resets_after_configured, 1);
    assert_eq!(b.log.take(), [Seen::Reset]);
    assert_eq!(stats(b.sim, 1).0, Stat::Disabled, "the reset disabled it");
    // a reset before configuration is not a lost configuration
    b.reset();
    assert_eq!(b.status().resets_after_configured, 1);
}

#[test]
fn deconfigure_and_reconfigure() {
    let mut b = Bench::new();
    b.enumerate();
    b.log.take();
    assert_eq!(b.control_none([0, 9, 0, 0, 0, 0, 0, 0]), Handshake::Ack);
    assert!(!b.status().configured);
    assert_eq!(stats(b.sim, 1).0, Stat::Disabled);
    assert_eq!(stats(b.sim, 2).1, Stat::Disabled);
    assert_eq!(b.control_none(SET_CONFIG_1), Handshake::Ack);
    assert_eq!(stats(b.sim, 1).0, Stat::Valid);
    assert_eq!(
        b.log.take(),
        [Seen::Configured(false), Seen::Configured(true)]
    );
    assert_eq!(b.status().resets_after_configured, 0);
}

#[test]
fn set_interface_enables_its_endpoints_only() {
    let mut b = Bench::new();
    b.enumerate();
    b.log.take();
    assert_eq!(b.control_none([1, 11, 1, 0, 1, 0, 0, 0]), Handshake::Ack);
    assert_eq!(b.log.take(), [Seen::Alt(1, 1)]);
    assert_eq!(stats(b.sim, 3).1, Stat::Nak, "setting 1 has the IN");
    assert_eq!(stats(b.sim, 1).0, Stat::Valid, "interface 0 untouched");
    assert_eq!(b.control_in([0x81, 10, 0, 0, 1, 0, 1, 0]).unwrap(), [1]);
    assert_eq!(b.control_none([1, 11, 0, 0, 1, 0, 0, 0]), Handshake::Ack);
    assert_eq!(stats(b.sim, 3).1, Stat::Disabled);
    // a setting the interface lacks stalls
    assert_eq!(b.control_none([1, 11, 2, 0, 1, 0, 0, 0]), Handshake::Stall);
}

#[test]
fn halt_and_clear() {
    let mut b = Bench::new();
    b.enumerate();
    // SET_FEATURE(ENDPOINT_HALT) on EP2 IN
    assert_eq!(b.control_none([2, 3, 0, 0, 0x82, 0, 0, 0]), Handshake::Ack);
    assert_eq!(b.sim.host_in(2), Err(Handshake::Stall));
    assert_eq!(
        b.control_in([0x82, 0, 0, 0, 0x82, 0, 2, 0]).unwrap(),
        [1, 0]
    );
    // CLEAR_FEATURE: NAK again (nothing to send), DATA0
    assert_eq!(b.control_none([2, 1, 0, 0, 0x82, 0, 0, 0]), Handshake::Ack);
    assert_eq!(b.sim.host_in(2), Err(Handshake::Nak));
    assert_eq!(
        b.control_in([0x82, 0, 0, 0, 0x82, 0, 2, 0]).unwrap(),
        [0, 0]
    );
}

#[test]
fn unknown_request_stalls_and_the_next_one_works() {
    let mut b = Bench::new();
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
fn class_write_with_data_stage() {
    let mut b = Bench::new();
    b.enumerate();
    b.log.take();
    let data: Vec<u8> = (0..40).collect();
    assert_eq!(
        b.control_out([0x41, 7, 0, 0, 0, 0, 40, 0], &data),
        Handshake::Ack
    );
    assert_eq!(b.log.take(), [Seen::Out(7, data)]);
}

// A host that drops a transfer and sends the next SETUP instead of the status stage. While the last IN data
// packet is armed, OUT is NAK and a SETUP gets no answer (R5, RM0444 §37.6.7: "If a control endpoint/channel is
// defined as NAK, the USB peripheral does not answer"), so the host retries it; once the data is sent, EP0 waits
// for the status OUT with OUT VALID, and the SETUP is taken and starts a new transfer.
#[test]
fn a_setup_instead_of_the_status_stage_starts_over() {
    let mut b = Bench::new();
    assert_eq!(
        b.sim.host_setup(0, [0x80, 6, 0, 2, 0, 0, 255, 0]),
        Handshake::Ack
    );
    b.pump();
    assert!(b.sim.host_in(0).is_ok());
    b.pump();
    assert_eq!(
        b.sim.host_setup(0, [0x80, 6, 0, 1, 0, 0, 18, 0]),
        Handshake::None,
        "last packet armed: OUT NAK, SETUP unanswered"
    );
    assert!(b.sim.host_in(0).is_ok());
    b.pump();
    assert_eq!(
        b.control_in([0x80, 6, 0, 1, 0, 0, 18, 0]).unwrap(),
        Desc::DEVICE_DESC
    );
    // and in the middle of a data stage (OUT STALL), a SETUP is taken at once
    assert_eq!(
        b.sim.host_setup(0, [0x80, 6, 0, 2, 0, 0, 255, 0]),
        Handshake::Ack
    );
    b.pump();
    assert_eq!(
        b.sim.chep_value(0) >> 12 & 3,
        0b01,
        "OUT STALL while not last"
    );
    assert_eq!(
        b.control_in([0x80, 6, 0, 1, 0, 0, 18, 0]).unwrap(),
        Desc::DEVICE_DESC
    );
}

#[test]
fn suspend_and_resume() {
    let mut b = Bench::new();
    b.enumerate();
    b.log.take();
    b.sim.suspend();
    b.pump();
    assert!(b.status().suspended);
    assert_eq!(b.log.take(), [Seen::Suspended(true)]);
    b.sim.wakeup();
    b.pump();
    assert!(!b.status().suspended);
    assert!(b.status().configured, "a resume keeps the configuration");
    assert_eq!(b.log.take(), [Seen::Suspended(false)]);
    // both before the task looked: it still hears the suspend, and ends resumed
    b.sim.suspend();
    interrupt(&b.sim, &Desc::LAYOUT, b.state.as_ref());
    b.sim.wakeup();
    b.pump();
    assert!(!b.status().suspended);
    assert_eq!(
        b.log.take(),
        [Seen::Suspended(true), Seen::Suspended(false)]
    );
}

#[test]
fn loopback_through_the_class_endpoints() {
    let mut b = Bench::new();
    let mut out = b.driver.bulk_out::<1, Double>();
    let mut inp = b.driver.bulk_in::<2, Double>();
    b.enumerate();
    let mut echo = Box::pin(async move {
        let mut buf = [0u8; 64];
        loop {
            let n = out.read(&mut buf).await.unwrap();
            inp.write(&buf[..n]).await.unwrap();
        }
    });
    let mut cx = Context::from_waker(Waker::noop());
    for k in 0..20u8 {
        let p: Vec<u8> = (0..64).map(|i| i ^ k).collect();
        assert_eq!(b.sim.host_out(1, &p), Handshake::Ack);
        b.pump();
        let _ = echo.as_mut().poll(&mut cx);
        b.pump();
        assert_eq!(b.sim.host_in(2).unwrap(), p);
        b.pump();
    }
    b.sim.with(|i| {
        assert!(i.r4.is_empty(), "R4: {:?}", i.r4);
        assert!(i.r6.is_empty(), "R6: {:?}", i.r6);
    });
}

#[test]
fn status_changed_wakes() {
    let mut b = Bench::new();
    let seen = b.status();
    let state = b.state;
    let mut f = Box::pin(state.status_changed(seen));
    let mut cx = Context::from_waker(Waker::noop());
    assert!(f.as_mut().poll(&mut cx).is_pending());
    b.enumerate();
    match f.as_mut().poll(&mut cx) {
        Poll::Ready(s) => assert!(s != seen),
        Poll::Pending => panic!("no change seen"),
    }
}
