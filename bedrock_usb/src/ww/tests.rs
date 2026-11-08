//! The WireWeaver class on the simulated peripheral: the descriptors the macro writes, and a packet each way.

extern crate std;

use core::future::{Future, pending, ready};
use core::pin::pin;
use core::task::{Context, Poll, Waker};
use std::boxed::Box;
use std::vec::Vec;

use ww_device::{PacketSink, PacketSource};
use ww_link::{USB_INTERFACE_PROTOCOL, USB_INTERFACE_PROTOCOL_SEQ, UsbSeqFraming};

use super::{MS_OS_20, Receiver, SELF_ID, Sender, Sleep};
use crate::Buffering::{Double, Single};
use crate::desc::{Device, Str};
use crate::v4::sim::{Handshake, Sim};
use crate::v4::{Driver, EndpointError, State};
use crate::{Direction, EndpointAddress};

const DEVICE: Device = Device {
    ms_os_20: Some(MS_OS_20),
    functions: &[
        crate::ww_function! {
            out: 1, in: 2, max_packet: 64, buffering: Double,
            user_crate: "test_api", user_version: (1, 2, 3), api_id: Str::Const("test_api 1.2.3"),
        },
        crate::ww_function! {
            out: 3, in: 3, max_packet: 64, buffering: Single,
            user_crate: "seq_api", user_version: (0, 1, 0), api_id: Str::None,
            framing: UsbSeqFraming,
        },
    ],
    ..Device::new(0xc0de, 0xcafe)
};
crate::descriptors!(Desc = DEVICE);

/// Never fires: the write waits as long as it needs.
struct Never;
impl Sleep for Never {
    fn sleep(&self) -> impl Future<Output = ()> {
        pending()
    }
}

/// Fires at once: a write that can't complete times out on the first poll.
struct Now;
impl Sleep for Now {
    fn sleep(&self) -> impl Future<Output = ()> {
        ready(())
    }
}

fn poll<F: Future>(f: F) -> Poll<F::Output> {
    poll_pinned(pin!(f))
}

fn poll_pinned<F: Future>(f: core::pin::Pin<&mut F>) -> Poll<F::Output> {
    f.poll(&mut Context::from_waker(Waker::noop()))
}

fn now<F: Future>(f: F) -> F::Output {
    match poll(f) {
        Poll::Ready(v) => v,
        Poll::Pending => panic!("not ready"),
    }
}

fn device() -> (&'static Sim, Driver<Desc, &'static Sim>) {
    let sim: &'static Sim = Box::leak(Box::new(Sim::new()));
    let state: &'static State<{ Desc::LAYOUT.count as usize }> = Box::leak(Box::new(State::new()));
    let d = Driver::new(sim, state);
    d.init();
    sim.bus_reset();
    d.on_interrupt();
    d.take_events(0xff);
    for (n, dir) in [
        (1, Direction::Out),
        (2, Direction::In),
        (3, Direction::Out),
        (3, Direction::In),
    ] {
        d.set_enabled(EndpointAddress::new(n, dir), true);
    }
    (sim, d)
}

/// The interface descriptors of the configuration: (`bInterfaceProtocol`, the descriptors that follow up to the next).
type Interfaces = Vec<(u8, Vec<(u8, Vec<u8>)>)>;

fn interfaces() -> Interfaces {
    let mut out = Vec::new();
    let mut rest: &[u8] = &Desc::CONFIG_DESC;
    while !rest.is_empty() {
        let (len, ty) = (rest[0] as usize, rest[1]);
        if ty == 4 {
            out.push((rest[7], Vec::new()));
        } else if ty == 11 {
            rest = &rest[len..];
            continue;
        } else if let Some((_, list)) = out.last_mut() {
            list.push((ty, rest[2..len].to_vec()));
        }
        rest = &rest[len..];
    }
    out
}

#[test]
fn function_descriptors() {
    let ifaces = interfaces();
    assert_eq!(ifaces.len(), 2);
    let (protocol, list) = &ifaces[0];
    assert_eq!(*protocol, USB_INTERFACE_PROTOCOL);
    let types: Vec<u8> = list.iter().map(|d| d.0).collect();
    assert_eq!(types, [5, 5, 0x40, 0x41, 0x42]);
    assert_eq!(list[2].1, SELF_ID);
    assert_eq!(&SELF_ID[..4], b"bdus");
    assert_eq!(
        SELF_ID[4..],
        [0, 1, 0],
        "this crate's version, Cargo.toml says 0.1.x"
    );
    assert_eq!(list[3].1, b"test_api");
    assert_eq!(list[4].1, [1, 2, 3]);
    assert_eq!(ifaces[1].0, USB_INTERFACE_PROTOCOL_SEQ);
    // MS OS 2.0: the device level and each function carry WinUSB and the GUID
    let set: &[u8] = &Desc::MS_OS_20_SET;
    let text = |s: &[u8]| s.windows(6).filter(|w| w == b"WINUSB").count();
    assert_eq!(text(set), 3, "device level and two function subsets");
}

#[test]
fn packets_each_way() {
    let (sim, d) = device();
    let mut tx: Sender<_, _> = Sender::new(d.bulk_in::<2, crate::buffer::Double>(), Never);
    let mut rx: Receiver<_> = Receiver::new(d.bulk_out::<1, crate::buffer::Double>());
    assert_eq!(tx.max_packet_size(), 64);
    assert_eq!(rx.max_packet_len(), 64);
    now(tx.write_packet(&[1, 2, 3])).unwrap();
    assert_eq!(sim.host_in(2).unwrap(), [1, 2, 3]);
    let mut buf = [0; 64];
    assert_eq!(sim.host_out(1, &[9, 8, 7, 6]), Handshake::Ack);
    d.on_interrupt();
    assert_eq!(now(rx.read_packet(&mut buf)), Ok(4));
    assert_eq!(buf[..4], [9, 8, 7, 6]);
    // nothing more: the read waits
    assert!(poll(rx.read_packet(&mut buf)).is_pending());
}

#[test]
fn write_times_out_when_the_host_does_not_read() {
    let (sim, d) = device();
    let mut tx: Sender<_, _, UsbSeqFraming> =
        Sender::new(d.bulk_in::<3, crate::buffer::Single>(), Now);
    now(tx.write_packet(&[1])).unwrap();
    // the host has not taken the first packet: the second waits, and the timer ends it
    assert_eq!(now(tx.write_packet(&[2])), Err(EndpointError::Disabled));
    assert_eq!(sim.host_in(3).unwrap(), [1]);
}

#[test]
fn waits_for_the_host_to_enable_the_interface() {
    let (_sim, d) = device();
    d.set_enabled(EndpointAddress::new(1, Direction::Out), false);
    let mut rx: Receiver<_> = Receiver::new(d.bulk_out::<1, crate::buffer::Double>());
    assert!(poll(rx.wait_connected()).is_pending());
    d.set_enabled(EndpointAddress::new(1, Direction::Out), true);
    assert!(poll(rx.wait_connected()).is_ready());
}
