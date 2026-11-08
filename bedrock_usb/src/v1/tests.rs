//! [Mmio] against memory laid out as the usb_v1 peripheral (registers, then USB SRAM): with [Spaced512] every
//! half-word of the packet memory lands at the start of a 32-bit word, as RM0008 §23.5.3 puts it ("the actual
//! memory location address must be multiplied by two"); with [Packed512] / [Packed1024] at consecutive addresses.
//! The driver's start-up leaves the buffer table the hardware reads, the pull-up goes where [PullUp] says, and
//! [on_interrupt] serves the registered driver.

extern crate std;

use core::cell::UnsafeCell;
use std::boxed::Box;

use super::{Access, Driver, Mmio, Packed512, Packed1024, PullUp, Regs, Spaced512, State};
use crate::Buffering;
use crate::desc::{Device, Endpoint, Function, Interface, Item};
use crate::v4::endpoint::{read_packet, write_packet};
use crate::v4::regs::{cntr, daddr, istr};

/// The peripheral's registers (offsets 0x00 to 0x5c), its USB SRAM as the CPU sees it (1024 bytes: 512 spaced or
/// 1024 packed), and a GPIO port's BSRR, as plain memory.
#[repr(C, align(4))]
struct Chip {
    regs: [u32; 24],
    sram: [u16; 512],
    bsrr: u32,
}

/// A chip of zeros, leaked, and an [Mmio] on it.
fn chip<A: Access>(pull_up: impl FnOnce(usize) -> PullUp) -> (&'static UnsafeCell<Chip>, Mmio<A>) {
    let c: &'static UnsafeCell<Chip> = Box::leak(Box::new(UnsafeCell::new(Chip {
        regs: [0; 24],
        sram: [0; 512],
        bsrr: 0,
    })));
    let p = c.get();
    // Safety: the memory is ours and outlives the Mmio; every access goes through raw pointers
    let m = unsafe {
        Mmio::at(
            core::ptr::addr_of_mut!((*p).regs) as usize,
            core::ptr::addr_of_mut!((*p).sram) as usize,
            72_000_000,
            pull_up(core::ptr::addr_of_mut!((*p).bsrr) as usize),
        )
    };
    (c, m)
}

fn reg(c: &UnsafeCell<Chip>, offset: usize) -> u32 {
    unsafe { core::ptr::addr_of!((*c.get()).regs[offset / 4]).read_volatile() }
}

fn set_reg(c: &UnsafeCell<Chip>, offset: usize, value: u32) {
    unsafe { core::ptr::addr_of_mut!((*c.get()).regs[offset / 4]).write_volatile(value) }
}

/// The CPU's half-word `i` of the SRAM window (bytes `2i`, `2i + 1`).
fn cpu_half(c: &UnsafeCell<Chip>, i: usize) -> u16 {
    unsafe { core::ptr::addr_of!((*c.get()).sram[i]).read_volatile() }
}

fn bsrr(c: &UnsafeCell<Chip>) -> u32 {
    unsafe { core::ptr::addr_of!((*c.get()).bsrr).read_volatile() }
}

/// The packet memory's half-word `k` (the peripheral's bytes `2k`, `2k + 1`) through a [Spaced512] window.
fn spaced(c: &UnsafeCell<Chip>, k: usize) -> u16 {
    cpu_half(c, 2 * k)
}

#[test]
fn spaced_half_words_sit_at_the_start_of_each_word() {
    let (c, m) = chip::<Spaced512>(|_| PullUp::External);
    m.set_sram(3, 0xaabb_ccdd);
    // packet memory half-words 6 and 7, at CPU bytes 24 and 28
    assert_eq!((cpu_half(c, 12), cpu_half(c, 14)), (0xccdd, 0xaabb));
    assert_eq!(m.sram(3), 0xaabb_ccdd);
    // the upper halves of those words, and the neighbours, are left alone
    for i in [10, 11, 13, 15, 16] {
        assert_eq!(cpu_half(c, i), 0, "CPU half-word {i}");
    }
}

#[test]
fn packed_half_words_are_consecutive() {
    let (c, m) = chip::<Packed512>(|_| PullUp::External);
    m.set_sram(3, 0xaabb_ccdd);
    assert_eq!((cpu_half(c, 6), cpu_half(c, 7)), (0xccdd, 0xaabb));
    assert_eq!(m.sram(3), 0xaabb_ccdd);
    assert_eq!(<Mmio<Packed512> as Regs>::SRAM_SIZE, 512);
    assert_eq!(<Mmio<Packed1024> as Regs>::SRAM_SIZE, 1024);
    assert_eq!(<Mmio<Spaced512> as Regs>::SRAM_SIZE, 512);
}

#[test]
fn register_writes_keep_to_the_low_half() {
    let (c, m) = chip::<Spaced512>(|_| PullUp::External);
    // usb_v4's invariant write has 1 in its host-mode rc_w0 bits (23, 25-30): reserved here
    m.set_chep(2, 0xffff_ffff);
    assert_eq!(reg(c, 0x08), 0xffff);
    m.set_istr(!(1 << 10));
    assert_eq!(reg(c, 0x44), 0xfbff);
    m.set_btable(0x1_0008);
    assert_eq!(reg(c, crate::v3::offset::BTABLE), 0x0008);
}

#[test]
fn packets_land_byte_for_byte() {
    let (c, m) = chip::<Spaced512>(|_| PullUp::External);
    let data: [u8; 7] = [1, 2, 3, 4, 5, 6, 7];
    write_packet(&m, 0x48, &data);
    // little-endian half-words from packet memory byte 0x48 (half-word 0x24), each at the start of a CPU word
    assert_eq!(
        [
            spaced(c, 0x24),
            spaced(c, 0x25),
            spaced(c, 0x26),
            spaced(c, 0x27)
        ],
        [0x0201, 0x0403, 0x0605, 0x0007]
    );
    let mut back = [0u8; 7];
    read_packet(&m, 0x48, &mut back);
    assert_eq!(back, data);
}

#[test]
fn gpio_pull_up_sets_and_resets_through_bsrr() {
    let gpio = |active_high| {
        move |bsrr| PullUp::Gpio {
            bsrr,
            pin: 15,
            active_high,
        }
    };
    let (c, m) = chip::<Spaced512>(gpio(true));
    m.set_pull_up(true);
    assert_eq!(bsrr(c), 1 << 15);
    m.set_pull_up(false);
    assert_eq!(bsrr(c), 1 << 31);
    let (c, m) = chip::<Spaced512>(gpio(false));
    m.set_pull_up(true);
    assert_eq!(bsrr(c), 1 << 31);
    m.set_pull_up(false);
    assert_eq!(bsrr(c), 1 << 15);
    // a fixed resistor: nothing is written anywhere
    let (c, m) = chip::<Spaced512>(|_| PullUp::External);
    m.set_pull_up(true);
    assert_eq!(bsrr(c), 0);
}

const DEVICE: Device = Device {
    functions: &[Function {
        interfaces: &[Interface {
            items: &[
                Item::Endpoint(Endpoint::bulk_out(1, 64, Buffering::Double)),
                Item::Endpoint(Endpoint::bulk_in(2, 64, Buffering::Single)),
                Item::Endpoint(Endpoint::bulk_out(2, 32, Buffering::Single)),
            ],
            ..Interface::vendor()
        }],
        ..Function::vendor()
    }],
    ..Device::new(0xc0de, 0xcafe)
};
crate::descriptors!(Desc = DEVICE);

#[test]
fn start_up_writes_the_table_the_hardware_reads() {
    let (c, m) = chip::<Spaced512>(|_| PullUp::External);
    let state: &'static State<{ Desc::LAYOUT.count as usize }> = Box::leak(Box::new(State::new()));
    let d = Driver::<Desc, Mmio>::new(m, state);
    d.init();
    assert_eq!(reg(c, 0x40), super::INTERRUPTS & 0xffff);
    assert_eq!(reg(c, 0x40) & cntr::USBRST, 0);
    assert_eq!(reg(c, crate::v3::offset::BTABLE), 0);
    assert_eq!(reg(c, 0x4c), daddr::EF);
    // EP0R: control (EP_TYPE 01), STAT_RX VALID, STAT_TX NAK
    assert_eq!(reg(c, 0x00), 0x3220);
    // RM0008 §23.5.3, entry n at local address n * 8: ADDRn_TX, COUNTn_TX, ADDRn_RX, COUNTn_RX (BL_SIZE bit 15,
    // NUM_BLOCK 14:10), each half-word at the start of a CPU word. The table is 24 bytes, EP0's 64 + 64 from 24
    let rx64 = 0x8000 | (1 << 10);
    let rx32 = 16 << 10;
    let entry = |n: usize| {
        [
            spaced(c, 4 * n),
            spaced(c, 4 * n + 1),
            spaced(c, 4 * n + 2),
            spaced(c, 4 * n + 3),
        ]
    };
    assert_eq!(entry(0), [24, 0, 88, rx64]);
    // EP1 double-buffered OUT: ADDR1_RX_0 / COUNT1_RX_0 in the transmit half, as RM0008 Figure 221 shows
    assert_eq!(entry(1), [152, rx64, 216, rx64]);
    assert_eq!(entry(2), [280, 0, 344, rx32]);
    assert_eq!(Desc::LAYOUT.end, 376);
    // nothing lands in the upper halves of the CPU words
    for k in 0..Desc::LAYOUT.end as usize / 2 {
        assert_eq!(cpu_half(c, 2 * k + 1), 0, "CPU half-word {}", 2 * k + 1);
    }
}

/// Two double-buffered 64-byte endpoints are what 512 bytes hold (see the compile-fail doctest on [super] for
/// three).
#[test]
fn two_double_buffered_endpoints_fit() {
    const EPS: &[Item] = &[
        Item::Endpoint(Endpoint::bulk_out(1, 64, Buffering::Double)),
        Item::Endpoint(Endpoint::bulk_in(2, 64, Buffering::Double)),
    ];
    const D2: Device = Device {
        functions: &[Function {
            interfaces: &[Interface {
                items: EPS,
                ..Interface::vendor()
            }],
            ..Function::vendor()
        }],
        ..Device::new(0xc0de, 0xcafe)
    };
    crate::descriptors!(Two = D2);
    assert_eq!(Two::LAYOUT.end, 3 * 8 + 2 * 64 + 2 * 128);
    let (_c, m) = chip::<Spaced512>(|_| PullUp::External);
    let state: &'static State<{ Two::LAYOUT.count as usize }> = Box::leak(Box::new(State::new()));
    Driver::<Two, Mmio>::new(m, state).init();
}

#[test]
fn on_interrupt_serves_the_registered_driver() {
    let (c, m) = chip::<Spaced512>(|_| PullUp::External);
    let state: &'static State<{ Desc::LAYOUT.count as usize }> = Box::leak(Box::new(State::new()));
    let d = Driver::<Desc, Mmio>::new(m, state);
    d.init();
    d.register();
    // a wake-up while suspended (plain memory has no rc_w0, so the flag stays written as cleared: one that sets
    // CTR, as RST_DCON's clear does, would keep the handler's transfer loop going)
    set_reg(c, 0x40, reg(c, 0x40) | cntr::SUSPEN);
    set_reg(c, 0x44, istr::WKUP);
    super::on_interrupt::<Spaced512>();
    assert_eq!(reg(c, 0x40) & cntr::SUSPEN, 0);
    assert_eq!(reg(c, 0x44), !istr::WKUP & 0xffff);
    assert_eq!(d.take_events(super::event::RESUME), super::event::RESUME);
}
