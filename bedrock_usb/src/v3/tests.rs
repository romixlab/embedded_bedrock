//! [Mmio] against memory laid out as the usb_v3 peripheral (registers, then 512 half-words of USB SRAM): every
//! word of the driver lands as two half-words where RM0440 §45.6.2 puts them, register writes keep to the low 16
//! bits, and the driver's start-up leaves the buffer table the hardware reads.

extern crate std;

use core::cell::UnsafeCell;
use std::boxed::Box;

use super::{Driver, Mmio, Regs, State};
use crate::Buffering;
use crate::desc::{Device, Endpoint, Function, Interface, Item};
use crate::v4::endpoint::{read_packet, write_packet};
use crate::v4::regs::{cntr, daddr};

/// The peripheral's registers (offsets 0x00 to 0x5c) and its USB SRAM, as plain memory.
#[repr(C, align(4))]
struct Chip {
    regs: [u32; 24],
    sram: [u16; 512],
}

/// A chip of zeros, leaked, and an [Mmio] on it.
fn chip() -> (&'static UnsafeCell<Chip>, Mmio) {
    let c: &'static UnsafeCell<Chip> = Box::leak(Box::new(UnsafeCell::new(Chip {
        regs: [0; 24],
        sram: [0; 512],
    })));
    let p = c.get();
    // Safety: the memory is ours and outlives the Mmio; every access goes through raw pointers
    let m = unsafe {
        Mmio::new(
            core::ptr::addr_of_mut!((*p).regs) as usize,
            core::ptr::addr_of_mut!((*p).sram) as usize,
            48_000_000,
        )
    };
    (c, m)
}

fn reg(c: &UnsafeCell<Chip>, offset: usize) -> u32 {
    unsafe { core::ptr::addr_of!((*c.get()).regs[offset / 4]).read_volatile() }
}

fn half(c: &UnsafeCell<Chip>, i: usize) -> u16 {
    unsafe { core::ptr::addr_of!((*c.get()).sram[i]).read_volatile() }
}

#[test]
fn a_word_is_two_half_words_low_first() {
    let (c, m) = chip();
    m.set_sram(3, 0xaabb_ccdd);
    assert_eq!((half(c, 6), half(c, 7)), (0xccdd, 0xaabb));
    assert_eq!(m.sram(3), 0xaabb_ccdd);
    // the neighbours are left alone
    assert_eq!((half(c, 5), half(c, 8)), (0, 0));
}

#[test]
fn register_writes_keep_to_the_low_half() {
    let (c, m) = chip();
    // usb_v4's invariant write has 1 in its host-mode rc_w0 bits (23, 25-30): reserved here
    m.set_chep(2, 0xffff_ffff);
    assert_eq!(reg(c, 0x08), 0xffff);
    m.set_istr(!(1 << 10));
    assert_eq!(reg(c, 0x44), 0xfbff);
    m.set_btable(0x1_0008);
    assert_eq!(reg(c, super::offset::BTABLE), 0x0008);
}

#[test]
fn the_frame_number_is_read_from_fnr() {
    let (c, m) = chip();
    unsafe { core::ptr::addr_of_mut!((*c.get()).regs[0x48 / 4]).write_volatile(0x27ff) };
    assert_eq!(m.fnr(), 0x27ff);
}

#[test]
fn packets_land_byte_for_byte() {
    let (c, m) = chip();
    let data: [u8; 7] = [1, 2, 3, 4, 5, 6, 7];
    write_packet(&m, 0x48, &data);
    // little-endian half-words from byte 0x48 (half-word 0x24), as the peripheral uses them (RM0440 §45.5.2
    // "Structure and usage of packet buffers": "the first byte received is stored as least significant byte")
    assert_eq!(
        [half(c, 0x24), half(c, 0x25), half(c, 0x26), half(c, 0x27)],
        [0x0201, 0x0403, 0x0605, 0x0007]
    );
    let mut back = [0u8; 7];
    read_packet(&m, 0x48, &mut back);
    assert_eq!(back, data);
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
    let (c, m) = chip();
    let state: &'static State<{ Desc::LAYOUT.count as usize }> = Box::leak(Box::new(State::new()));
    let d = Driver::<Desc, Mmio>::new(m, state);
    d.init();
    assert_eq!(reg(c, 0x40), super::INTERRUPTS & 0xffff);
    assert_eq!(reg(c, 0x40) & cntr::USBRST, 0);
    assert_eq!(reg(c, super::offset::BTABLE), 0);
    assert_eq!(reg(c, 0x4c), daddr::EF);
    // EP0R: control (EP_TYPE 01), STAT_RX VALID, STAT_TX NAK, nothing above bit 15
    assert_eq!(reg(c, 0x00), 0x3220);
    // RM0440 §45.6.2, entry n at n * 8: ADDRn_TX, COUNTn_TX, ADDRn_RX, COUNTn_RX (BL_SIZE bit 15, NUM_BLOCK 14:10).
    // Three registers: the table is 24 bytes, EP0's 64 + 64 from 24
    let rx64 = 0x8000 | (1 << 10);
    let rx32 = 16 << 10;
    let entry = |n: usize| {
        [
            half(c, 4 * n),
            half(c, 4 * n + 1),
            half(c, 4 * n + 2),
            half(c, 4 * n + 3),
        ]
    };
    assert_eq!(entry(0), [24, 0, 88, rx64]);
    // EP1 double-buffered OUT: both halves of the entry are receive buffers (ADDRn_RX_0 / COUNTn_RX_0 first)
    assert_eq!(entry(1), [152, rx64, 216, rx64]);
    // EP2: IN 64 in the TX half, OUT 32 in the RX half
    assert_eq!(entry(2), [280, 0, 344, rx32]);
    assert_eq!(Desc::LAYOUT.end, 376);
}

/// The most a full-speed device asks for (seven double-buffered 64-byte endpoints) is 1088 bytes: more than
/// usb_v3's 1024, so six is where it ends (see the compile-fail doctest on [super]).
#[test]
fn six_double_buffered_endpoints_fit() {
    const EPS: &[Item] = &[
        Item::Endpoint(Endpoint::bulk_out(1, 64, Buffering::Double)),
        Item::Endpoint(Endpoint::bulk_in(2, 64, Buffering::Double)),
        Item::Endpoint(Endpoint::bulk_out(3, 64, Buffering::Double)),
        Item::Endpoint(Endpoint::bulk_in(4, 64, Buffering::Double)),
        Item::Endpoint(Endpoint::bulk_out(5, 64, Buffering::Double)),
        Item::Endpoint(Endpoint::bulk_in(6, 64, Buffering::Double)),
    ];
    const D6: Device = Device {
        functions: &[Function {
            interfaces: &[Interface {
                items: EPS,
                ..Interface::vendor()
            }],
            ..Function::vendor()
        }],
        ..Device::new(0xc0de, 0xcafe)
    };
    crate::descriptors!(Six = D6);
    assert_eq!(Six::LAYOUT.end, 7 * 8 + 2 * 64 + 6 * 128);
    let (_c, m) = chip();
    let state: &'static State<{ Six::LAYOUT.count as usize }> = Box::leak(Box::new(State::new()));
    Driver::<Six, Mmio>::new(m, state).init();
}
