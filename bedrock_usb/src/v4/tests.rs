//! The hardware rules as tests on the simulated peripheral ([super::sim]).

extern crate std;

use core::future::Future;
use core::pin::{Pin, pin};
use core::task::{Context, Poll, Waker};
use std::boxed::Box;
use std::vec::Vec;

use super::chep::{Stat, stat_rx, stat_tx};
use super::regs::{chep::*, cntr, daddr};
use super::sim::{Handshake, Op, Sim};
use super::{Driver, EndpointError, Ep0Error, State, event};
use crate::buffer::{Double, Single};
use crate::desc::{Device, Endpoint, Function, Interface, Item};
use crate::{Buffering, Descriptors, Direction, EndpointAddress};

const DEVICE: Device = Device {
    functions: &[Function {
        interfaces: &[Interface {
            items: &[
                Item::Endpoint(Endpoint::bulk_out(1, 64, Buffering::Double)),
                Item::Endpoint(Endpoint::bulk_in(2, 64, Buffering::Double)),
                Item::Endpoint(Endpoint::bulk_out(3, 64, Buffering::Single)),
                Item::Endpoint(Endpoint::bulk_in(3, 64, Buffering::Single)),
                Item::Endpoint(Endpoint::interrupt_in(4, 8, 10)),
            ],
            ..Interface::vendor()
        }],
        ..Function::vendor()
    }],
    ..Device::new(0xc0de, 0xcafe)
};
crate::descriptors!(Desc = DEVICE);

type Drv = Driver<Desc, &'static Sim>;

const OUT1: EndpointAddress = EndpointAddress::new(1, Direction::Out);
const IN2: EndpointAddress = EndpointAddress::new(2, Direction::In);
const OUT3: EndpointAddress = EndpointAddress::new(3, Direction::Out);
const IN3: EndpointAddress = EndpointAddress::new(3, Direction::In);
const IN4: EndpointAddress = EndpointAddress::new(4, Direction::In);

/// A started driver on a fresh chip, after a bus reset, with the class endpoints enabled.
fn device() -> (&'static Sim, Drv) {
    let sim: &'static Sim = Box::leak(Box::new(Sim::new()));
    let state: &'static State<{ Desc::LAYOUT.count as usize }> = Box::leak(Box::new(State::new()));
    let d = Driver::new(sim, state);
    d.init();
    sim.bus_reset();
    d.on_interrupt();
    assert_eq!(d.take_events(0xff), event::RESET);
    for a in [OUT1, IN2, OUT3, IN3, IN4] {
        d.set_enabled(a, true);
    }
    sim.take_log();
    (sim, d)
}

fn poll<F: Future>(f: Pin<&mut F>) -> Poll<F::Output> {
    f.poll(&mut Context::from_waker(Waker::noop()))
}

/// Polls `f` once and expects it done.
fn now<F: Future>(f: F) -> F::Output {
    match poll(pin!(f)) {
        Poll::Ready(v) => v,
        Poll::Pending => panic!("not ready"),
    }
}

/// One poll of a read: `None` while no packet is there.
fn try_read<const EP: u8>(
    out: &mut super::BulkOut<&'static Sim, EP, Double>,
    buf: &mut [u8],
) -> Option<Result<usize, EndpointError>> {
    match poll(pin!(out.read(buf))) {
        Poll::Ready(r) => Some(r),
        Poll::Pending => None,
    }
}

fn packet(seed: u8, len: usize) -> Vec<u8> {
    (0..len).map(|i| seed.wrapping_add(i as u8)).collect()
}

fn chep_writes(log: &[Op]) -> Vec<(usize, u32)> {
    log.iter()
        .filter_map(|o| match o {
            Op::Chep { n, value, .. } => Some((*n, *value)),
            _ => None,
        })
        .collect()
}

// R9: PDWN cleared with USBRST still set, tSTARTUP, USBRST cleared, ISTR cleared, then the table.
#[test]
fn r9_start_up() {
    let sim: &'static Sim = Box::leak(Box::new(Sim::new()));
    let state: &'static State<{ Desc::LAYOUT.count as usize }> = Box::leak(Box::new(State::new()));
    let d: Drv = Driver::new(sim, state);
    d.init();
    let log = sim.take_log();
    assert_eq!(
        log[0],
        Op::Cntr(cntr::USBRST),
        "transceiver on (PDWN 0), still in reset"
    );
    assert!(
        matches!(log[1], Op::Delay(ns) if ns >= 1000),
        "tSTARTUP 1 µs: {:?}",
        log[1]
    );
    match log[2] {
        Op::Cntr(v) => assert_eq!(v & (cntr::USBRST | cntr::PDWN), 0),
        o => panic!("{o:?}"),
    }
    assert_eq!(log[3], Op::Istr(0));
    // EP0 and the table as a bus reset leaves them
    let c = sim.chep_value(0);
    assert_eq!((stat_rx(c), stat_tx(c)), (Stat::Valid, Stat::Nak));
    assert_eq!((c & UTYPE) >> UTYPE_SHIFT, 0b01);
    sim.with(|i| {
        for n in 0..Desc::LAYOUT.count as usize {
            assert_eq!(i.sram[2 * n..2 * n + 2], Desc::LAYOUT.regs[n].bd);
        }
        assert_eq!(i.daddr, daddr::EF);
    });
    assert_eq!(
        d.take_events(0xff),
        0,
        "no reset news before the host resets the bus"
    );
}

// R8: the bus reset is handled in the interrupt handler, no task involved: EP0, the table and the address.
#[test]
fn r8_bus_reset_in_the_handler() {
    let (sim, d) = device();
    d.set_address(9);
    sim.with(|i| i.sram[..16].fill(0xdead_beef));
    sim.bus_reset();
    assert!(sim.irq());
    d.on_interrupt();
    assert!(!sim.irq(), "every flag handled");
    let c0 = sim.chep_value(0);
    assert_eq!(
        (stat_rx(c0), stat_tx(c0), c0 & EA),
        (Stat::Valid, Stat::Nak, 0)
    );
    for n in 1..5 {
        let c = sim.chep_value(n);
        assert_eq!((c & EA) as usize, n);
        assert_eq!((stat_rx(c), stat_tx(c)), (Stat::Disabled, Stat::Disabled));
        assert_eq!(c & EPKIND != 0, n <= 2, "DBL_BUF on 1 and 2");
    }
    sim.with(|i| {
        for n in 0..5 {
            assert_eq!(i.sram[2 * n..2 * n + 2], Desc::LAYOUT.regs[n].bd);
        }
        assert_eq!(i.daddr, daddr::EF, "address 0, function enabled");
    });
    assert_eq!(d.take_events(0xff), event::RESET);
    // an endpoint waiting across the reset ends with Disabled
    let mut out = d.bulk_out::<3, Single>();
    let mut buf = [0; 64];
    assert_eq!(now(out.read(&mut buf)), Err(EndpointError::Disabled));
}

// R1: a flag the hardware sets between the driver's read and its write survives the write.
#[test]
fn r1_no_read_modify_write() {
    let (sim, d) = device();
    // halting EP3 IN while a packet of EP3 OUT completes in between
    sim.take_log();
    sim.race(|i| i.chep[3] |= VTRX);
    d.set_stalled(IN3, true);
    assert_ne!(sim.chep_value(3) & VTRX, 0, "VTRX kept");
    assert_eq!(stat_tx(sim.chep_value(3)), Stat::Stall);
    // the handler clearing VTRX while an IN of the same register completes
    sim.take_log();
    sim.race(|i| i.chep[3] |= VTTX);
    d.on_interrupt();
    // the handler loops on CTR, so it saw VTTX too and cleared it on purpose, with its own write
    let log = sim.take_log();
    let writes = chep_writes(&log);
    assert!(writes.len() >= 2, "{log:?}");
    assert_eq!(
        writes[0].1 & VTTX,
        VTTX,
        "the write clearing VTRX had 1 in VTTX"
    );
    assert_eq!(writes[0].1 & VTRX, 0);
    // every write of the driver: 1 in each rc_w0 bit it doesn't clear, nothing toggled by accident
    let (sim, d) = device();
    d.set_enabled(OUT3, false);
    d.set_enabled(OUT3, true);
    for (_, v) in chep_writes(&sim.take_log()) {
        assert_eq!(v & HOST_RC_W0, HOST_RC_W0);
    }
}

// R1: a halt lands even when a transfer completes between the read and the write and moves STAT.
#[test]
fn r1_halt_retries() {
    let (sim, d) = device();
    let mut inp = d.bulk_in::<3, Single>();
    now(inp.write(&[1, 2, 3])).unwrap();
    assert_eq!(stat_tx(sim.chep_value(3)), Stat::Valid);
    // the host takes the packet right between the read and the write: STAT goes VALID -> NAK under the write,
    // whose toggles were computed for VALID -> STALL, so it lands on DISABLED; the loop goes on to STALL
    sim.race(|i| i.chep[3] = (i.chep[3] & !STATTX) | (Stat::Nak as u32) << STATTX_SHIFT | VTTX);
    d.set_stalled(IN3, true);
    assert_eq!(stat_tx(sim.chep_value(3)), Stat::Stall);
    assert!(d.is_stalled(IN3));
}

// R3, single-buffered IN: the interrupt flag cleared, then the buffer, then COUNT, then VALID.
#[test]
fn r3_single_in_order() {
    let (sim, d) = device();
    let mut inp = d.bulk_in::<3, Single>();
    now(inp.write(&packet(0, 64))).unwrap();
    // sent; the handler hasn't run, VTTX still set
    assert_eq!(sim.host_in(3).unwrap(), packet(0, 64));
    sim.take_log();
    now(inp.write(&packet(100, 10))).unwrap();
    let log = sim.take_log();
    let pos = |f: &dyn Fn(&Op) -> bool| log.iter().position(f).unwrap();
    let clear = pos(&|o| matches!(o, Op::Chep { value, .. } if value & VTTX == 0));
    let buf = Desc::LAYOUT.regs[3].buffer[0] as usize / 4;
    let data = pos(&|o| matches!(o, Op::SramWrite { index, .. } if *index == buf));
    let count = pos(&|o| matches!(o, Op::SramWrite { index: 6, .. }));
    let valid = pos(&|o| matches!(o, Op::Chep { after, .. } if stat_tx(*after) == Stat::Valid));
    assert!(clear < data && data < count && count < valid, "{log:?}");
    assert_eq!(sim.host_in(3).unwrap(), packet(100, 10));
    // a zero-length packet
    d.on_interrupt();
    now(inp.write(&[])).unwrap();
    assert_eq!(sim.host_in(3).unwrap(), Vec::<u8>::new());
    // too long
    assert_eq!(now(inp.write(&[0; 65])), Err(EndpointError::BufferOverflow));
}

// R3 and R6, single-buffered OUT: flag cleared, the 800 ns wait, the count, the data, then VALID.
#[test]
fn r3_r6_single_out_order() {
    let (sim, d) = device();
    let mut out = d.bulk_out::<3, Single>();
    let mut buf = [0; 64];
    assert!(poll(pin!(out.read(&mut buf))).is_pending());
    assert_eq!(sim.host_out(3, &packet(7, 33)), Handshake::Ack);
    assert_eq!(sim.host_out(3, &[1]), Handshake::Nak, "NAK until read");
    sim.take_log();
    // read before the handler ran: the task sees STATRX NAK
    assert_eq!(now(out.read(&mut buf)), Ok(33));
    assert_eq!(buf[..33], packet(7, 33));
    let log = sim.take_log();
    let pos = |f: &dyn Fn(&Op) -> bool| log.iter().position(f).unwrap();
    let clear = pos(&|o| matches!(o, Op::Chep { value, .. } if value & VTRX == 0));
    let wait = pos(&|o| matches!(o, Op::Delay(ns) if *ns >= 800));
    let count = pos(&|o| matches!(o, Op::SramRead { index: 7 }));
    let valid = pos(&|o| matches!(o, Op::Chep { after, .. } if stat_rx(*after) == Stat::Valid));
    assert!(clear < wait && wait < count && count < valid, "{log:?}");
    sim.with(|i| assert!(i.r6.is_empty(), "SRAM read too early: {:?}", i.r6));
    // a packet longer than the buffer given stays
    assert_eq!(sim.host_out(3, &packet(1, 20)), Handshake::Ack);
    let mut small = [0; 8];
    assert_eq!(
        now(out.read(&mut small)),
        Err(EndpointError::BufferOverflow)
    );
    assert_eq!(now(out.read(&mut buf)), Ok(20));
}

// R7: a packet longer than the receive buffer gets a STALL and no interrupt; the buffer is the max packet size.
#[test]
fn r7_overrun_is_a_stall() {
    let (sim, d) = device();
    assert_eq!(sim.host_out(3, &[0; 65]), Handshake::Stall);
    assert!(!sim.irq(), "no interrupt");
    assert_eq!(
        stat_rx(sim.chep_value(3)),
        Stat::Valid,
        "the endpoint still takes the next packet"
    );
    assert_eq!(sim.host_out(3, &[0; 64]), Handshake::Ack);
    assert_eq!(
        sim.host_out(1, &[0; 65]),
        Handshake::Stall,
        "double-buffered too"
    );
    d.on_interrupt();
}

// R4, double-buffered IN: one buffer handed over at a time, the second by the handler; SW_BUF and STAT never in
// one write; packets in order, none lost or sent twice.
#[test]
fn r4_double_in() {
    let (sim, d) = device();
    let mut inp = d.bulk_in::<2, Double>();
    assert_eq!(sim.host_in(2), Err(Handshake::Nak), "nothing queued");
    now(inp.write(&packet(0, 64))).unwrap();
    now(inp.write(&packet(1, 64))).unwrap();
    assert!(
        poll(pin!(inp.write(&packet(2, 64)))).is_pending(),
        "both buffers queued"
    );
    let mut sent = Vec::new();
    let mut next = 2u8;
    for _ in 0..40 {
        match sim.host_in(2) {
            Ok(p) => sent.push(p),
            Err(Handshake::Nak) => {}
            Err(h) => panic!("{h:?}"),
        }
        d.on_interrupt();
        while next < 12 {
            match poll(pin!(inp.write(&packet(next, 64)))) {
                Poll::Ready(r) => r.unwrap(),
                Poll::Pending => break,
            }
            next += 1;
        }
        if sent.len() == 12 {
            break;
        }
    }
    let want: Vec<Vec<u8>> = (0..12).map(|s| packet(s, 64)).collect();
    assert_eq!(sent, want);
    sim.with(|i| assert!(i.r4.is_empty(), "SW_BUF and STAT in one write: {:?}", i.r4));
}

// R4, double-buffered OUT: the peripheral takes a second packet while the first waits; a third NAKs.
#[test]
fn r4_double_out() {
    let (sim, d) = device();
    let mut out = d.bulk_out::<1, Double>();
    let mut buf = [0; 64];
    assert_eq!(sim.host_out(1, &packet(0, 64)), Handshake::Ack);
    assert_eq!(
        sim.host_out(1, &packet(1, 64)),
        Handshake::Nak,
        "buffer 1 not handed over before the handler"
    );
    d.on_interrupt();
    assert_eq!(sim.host_out(1, &packet(1, 64)), Handshake::Ack);
    d.on_interrupt();
    assert_eq!(sim.host_out(1, &packet(2, 64)), Handshake::Nak, "both full");
    assert_eq!(now(out.read(&mut buf)), Ok(64));
    assert_eq!(buf, *packet(0, 64));
    assert_eq!(
        sim.host_out(1, &packet(2, 5)),
        Handshake::Ack,
        "buffer 0 free again"
    );
    d.on_interrupt();
    assert_eq!(now(out.read(&mut buf)), Ok(64));
    assert_eq!(buf, *packet(1, 64));
    assert_eq!(now(out.read(&mut buf)), Ok(5));
    assert_eq!(buf[..5], packet(2, 5));
    assert!(poll(pin!(out.read(&mut buf))).is_pending());
    sim.with(|i| {
        assert!(i.r4.is_empty(), "SW_BUF and STAT in one write: {:?}", i.r4);
        assert!(i.r6.is_empty(), "SRAM read too early: {:?}", i.r6);
    });
    // a long stream, the handler after each packet
    let mut got = Vec::new();
    let mut s = 10u8;
    for _ in 0..60 {
        if sim.host_out(1, &packet(s, 64)) == Handshake::Ack {
            s += 1;
        }
        d.on_interrupt();
        if s.is_multiple_of(3) {
            while let Some(r) = try_read(&mut out, &mut buf) {
                assert_eq!(r, Ok(64));
                got.push(buf[0]);
            }
        }
    }
    while let Some(r) = try_read(&mut out, &mut buf) {
        assert_eq!(r, Ok(64));
        got.push(buf[0]);
    }
    assert_eq!(got, (10..s).collect::<Vec<u8>>());
}

const GET_DEVICE: [u8; 8] = [0x80, 6, 0, 1, 0, 0, 18, 0];

// R5: a control read. SETUP sets both STAT to NAK; the last IN stage with OUT NAK; the status OUT with
// STATUS_OUT takes only a zero-length packet.
#[test]
fn r5_control_read() {
    let (sim, d) = device();
    let mut ep0 = d.ep0();
    assert_eq!(sim.host_setup(0, GET_DEVICE), Handshake::Ack);
    let c = sim.chep_value(0);
    assert_eq!((stat_rx(c), stat_tx(c)), (Stat::Nak, Stat::Nak));
    assert_eq!(
        sim.host_setup(0, GET_DEVICE),
        Handshake::None,
        "dropped while VTRX is pending"
    );
    d.on_interrupt();
    let s = now(ep0.setup());
    assert_eq!(s, crate::Setup::parse(&GET_DEVICE));
    sim.with(|i| assert!(i.r6.is_empty(), "SETUP read too early"));

    {
        let mut f = pin!(ep0.data_in(&Desc::DEVICE_DESC, true));
        assert!(poll(f.as_mut()).is_pending());
        let c = sim.chep_value(0);
        assert_eq!(
            (stat_rx(c), stat_tx(c)),
            (Stat::Nak, Stat::Valid),
            "last stage: OUT NAK"
        );
        assert_eq!(
            sim.host_out(0, &[]),
            Handshake::Nak,
            "status too early waits"
        );
        assert_eq!(sim.host_in(0).unwrap(), Desc::DEVICE_DESC);
        d.on_interrupt();
        assert_eq!(poll(f), Poll::Ready(Ok(())));
    }
    {
        let mut f = pin!(ep0.status_out());
        assert!(poll(f.as_mut()).is_pending());
        assert_ne!(sim.chep_value(0) & EPKIND, 0, "STATUS_OUT");
        assert_eq!(
            sim.host_out(0, &[1]),
            Handshake::Stall,
            "STATUS_OUT: data in the status stage stalls"
        );
        assert_eq!(sim.host_out(0, &[]), Handshake::Ack);
        assert_eq!(
            poll(f.as_mut()),
            Poll::Ready(Ok(())),
            "done before the handler ran"
        );
    }
    d.on_interrupt();
    // the handler armed EP0 for the next SETUP: STATUS_OUT off, OUT VALID
    let c = sim.chep_value(0);
    assert_eq!((c & EPKIND, stat_rx(c)), (0, Stat::Valid));
    assert_eq!(sim.host_setup(0, GET_DEVICE), Handshake::Ack);
    d.on_interrupt();
    now(ep0.setup());
    {
        // and the status done after the handler ran
        let mut f = pin!(ep0.status_out());
        assert!(poll(f.as_mut()).is_pending());
        sim.host_out(0, &[]);
        d.on_interrupt();
        assert_eq!(poll(f), Poll::Ready(Ok(())));
    }
}

// R5: a data stage before the last has the other direction STALL; a SETUP in the middle ends it.
#[test]
fn r5_multi_packet_and_setup_abort() {
    let (sim, d) = device();
    let mut ep0 = d.ep0();
    sim.host_setup(0, [0x80, 6, 0, 2, 0, 0, 255, 0]);
    d.on_interrupt();
    now(ep0.setup());
    {
        let mut f = pin!(ep0.data_in(&[0; 64], false));
        assert!(poll(f.as_mut()).is_pending());
        assert_eq!(
            stat_rx(sim.chep_value(0)),
            Stat::Stall,
            "not the last stage: OUT STALL"
        );
        assert_eq!(
            sim.host_out(0, &[]),
            Handshake::Stall,
            "a host ending early gets a STALL"
        );
        assert_eq!(sim.host_in(0).unwrap().len(), 64);
        d.on_interrupt();
        assert_eq!(poll(f), Poll::Ready(Ok(())));
    }
    {
        // the next stage is armed and the host sends a new SETUP instead (STALL in the receive direction takes it)
        let mut f = pin!(ep0.data_in(&[0; 64], false));
        assert!(poll(f.as_mut()).is_pending());
        assert_eq!(sim.host_setup(0, GET_DEVICE), Handshake::Ack);
        // seen in the register before the handler has run
        assert_eq!(poll(f.as_mut()), Poll::Ready(Err(Ep0Error::Setup)));
    }
    d.on_interrupt();
    assert_eq!(now(ep0.setup()), crate::Setup::parse(&GET_DEVICE));
}

// R5: a control write: OUT data, then the status IN; SET_ADDRESS after it (R14).
#[test]
fn r5_control_write_and_address() {
    let (sim, d) = device();
    let mut ep0 = d.ep0();
    sim.host_setup(0, [0x41, 1, 0, 0, 0, 0, 3, 0]);
    d.on_interrupt();
    now(ep0.setup());
    let mut buf = [0; 64];
    {
        let mut f = pin!(ep0.data_out(&mut buf, true));
        assert!(poll(f.as_mut()).is_pending());
        assert_eq!(
            stat_tx(sim.chep_value(0)),
            Stat::Nak,
            "last OUT stage: IN NAK"
        );
        assert_eq!(sim.host_out(0, &[1, 2, 3]), Handshake::Ack);
        assert_eq!(poll(f), Poll::Ready(Ok(3)));
    }
    assert_eq!(buf[..3], [1, 2, 3]);
    d.on_interrupt();
    {
        let mut f = pin!(ep0.status_in());
        assert!(poll(f.as_mut()).is_pending());
        assert_eq!(
            stat_rx(sim.chep_value(0)),
            Stat::Stall,
            "status IN: OUT STALL"
        );
        assert_eq!(sim.host_in(0).unwrap(), Vec::<u8>::new());
        d.on_interrupt();
        assert_eq!(poll(f), Poll::Ready(Ok(())));
    }
    d.set_address(5);
    sim.with(|i| assert_eq!(i.daddr, daddr::EF | 5));
    // a bus reset ends a waiting transfer
    sim.host_setup(0, [0x00, 9, 1, 0, 0, 0, 0, 0]);
    d.on_interrupt();
    now(ep0.setup());
    {
        let mut f = pin!(ep0.status_in());
        assert!(poll(f.as_mut()).is_pending());
        sim.bus_reset();
        d.on_interrupt();
        assert_eq!(poll(f), Poll::Ready(Err(Ep0Error::Reset)));
    }
    ep0.stall();
    let c = sim.chep_value(0);
    assert_eq!((stat_rx(c), stat_tx(c)), (Stat::Stall, Stat::Stall));
    assert_eq!(
        sim.host_setup(0, GET_DEVICE),
        Handshake::Ack,
        "a SETUP is taken while stalled"
    );
}

// R12: clearing a halt, and enabling, resets the data toggle to DATA0, halted or not; for a double-buffered
// endpoint the buffer selectors too.
#[test]
fn r12_data_toggle_reset() {
    let (sim, d) = device();
    let mut out = d.bulk_out::<3, Single>();
    let mut buf = [0; 64];
    sim.host_out(3, &[1]);
    now(out.read(&mut buf)).unwrap();
    assert_ne!(sim.chep_value(3) & DTOGRX, 0, "DATA1 next");
    d.set_stalled(OUT3, false); // ClearFeature on an endpoint that isn't halted
    assert_eq!(sim.chep_value(3) & DTOGRX, 0);
    assert_eq!(stat_rx(sim.chep_value(3)), Stat::Valid);
    d.set_stalled(OUT3, true);
    assert_eq!(sim.host_out(3, &[1]), Handshake::Stall);
    d.set_stalled(OUT3, false);
    assert_eq!(
        (stat_rx(sim.chep_value(3)), sim.chep_value(3) & DTOGRX),
        (Stat::Valid, 0)
    );

    let mut inp = d.bulk_in::<3, Single>();
    now(inp.write(&[1])).unwrap();
    sim.host_in(3).unwrap();
    d.on_interrupt();
    assert_ne!(sim.chep_value(3) & DTOGTX, 0);
    d.set_enabled(IN3, true); // SET_CONFIGURATION again
    assert_eq!(sim.chep_value(3) & DTOGTX, 0);

    // double-buffered OUT: DTOG 0, SW_BUF 1, the queue empty
    sim.host_out(1, &[1]);
    d.on_interrupt();
    sim.host_out(1, &[2]);
    d.on_interrupt();
    d.set_stalled(OUT1, true);
    assert_eq!(sim.host_out(1, &[3]), Handshake::Stall);
    d.set_stalled(OUT1, false);
    let c = sim.chep_value(1);
    assert_eq!(
        (c & DTOGRX, c & DTOGTX, stat_rx(c)),
        (0, DTOGTX, Stat::Valid)
    );
    let mut dout = d.bulk_out::<1, Double>();
    assert!(
        poll(pin!(dout.read(&mut buf))).is_pending(),
        "packets from before the halt dropped"
    );
    assert_eq!(sim.host_out(1, &[9]), Handshake::Ack);
    d.on_interrupt();
    assert_eq!(now(dout.read(&mut buf)), Ok(1));
    assert_eq!(buf[0], 9);

    // double-buffered IN: DTOG 0, SW_BUF 0
    let mut din = d.bulk_in::<2, Double>();
    now(din.write(&[1])).unwrap();
    sim.host_in(2).unwrap();
    d.on_interrupt();
    d.set_stalled(IN2, false);
    let c = sim.chep_value(2);
    assert_eq!((c & DTOGTX, c & DTOGRX, stat_tx(c)), (0, 0, Stat::Nak));
    now(din.write(&[2])).unwrap();
    assert_eq!(sim.host_in(2).unwrap(), [2]);
    sim.with(|i| assert!(i.r4.is_empty(), "{:?}", i.r4));
}

#[test]
fn disable_ends_waits_and_halt_waits() {
    let (sim, d) = device();
    let mut inp = d.interrupt_in::<4>();
    now(inp.write(&[1; 8])).unwrap();
    assert_eq!(sim.host_in(4).unwrap(), [1; 8]);
    d.set_stalled(IN4, true);
    assert!(
        poll(pin!(inp.write(&[2]))).is_pending(),
        "halted: waits for the host to clear it"
    );
    d.set_stalled(IN4, false);
    now(inp.write(&[2])).unwrap();
    d.set_enabled(IN4, false);
    assert!(sim.host_in(4) == Err(Handshake::None));
    assert_eq!(now(inp.write(&[3])), Err(EndpointError::Disabled));
    assert!(poll(pin!(inp.wait_enabled())).is_pending());
    d.set_enabled(IN4, true);
    now(inp.wait_enabled());
}

#[test]
fn suspend_and_wake_up() {
    let (sim, d) = device();
    sim.suspend();
    d.on_interrupt();
    assert_ne!(sim.with(|i| i.cntr) & cntr::SUSPEN, 0);
    assert_eq!(d.take_events(0xff), event::SUSPEND);
    sim.with(|i| i.cntr |= cntr::SUSPRDY); // the firmware lowered power further
    sim.wakeup();
    d.on_interrupt();
    assert_eq!(sim.with(|i| i.cntr) & (cntr::SUSPEN | cntr::SUSPRDY), 0);
    assert_eq!(
        now(d.wait_events(event::RESUME | event::RESET)),
        event::RESUME
    );
    assert!(!sim.irq());
    // nothing pending: the handler only reads ISTR
    sim.take_log();
    d.on_interrupt();
    assert!(sim.take_log().is_empty());
}

#[test]
fn connect() {
    let (sim, d) = device();
    d.connect();
    assert_ne!(sim.with(|i| i.bcdr) & super::regs::bcdr::DPPU, 0);
    d.disconnect();
    assert_eq!(sim.with(|i| i.bcdr), 0);
}

#[test]
#[should_panic(expected = "handed out twice")]
fn one_handle_per_endpoint() {
    let (_, d) = device();
    let _a = d.bulk_out::<3, Single>();
    let _b = d.bulk_out::<3, Single>();
}

#[test]
fn word_copies() {
    let sim: &'static Sim = Box::leak(Box::new(Sim::new()));
    for len in 0..12 {
        let data = packet(len as u8, len);
        super::endpoint::write_packet(&sim, 64, &data);
        let mut back = std::vec![0; len];
        super::endpoint::read_packet(&sim, 64, &mut back);
        assert_eq!(back, data);
        // whole words: ceil(len / 4) writes, each one word
        let log = sim.take_log();
        assert_eq!(
            log.iter()
                .filter(|o| matches!(o, Op::SramWrite { .. }))
                .count(),
            len.div_ceil(4)
        );
    }
}

#[test]
fn layout_matches_the_device() {
    assert_eq!(Desc::LAYOUT.count, 5);
    assert!(Desc::has(Direction::Out, 1, Buffering::Double));
    let _ = <Desc as Descriptors>::LAYOUT;
}
