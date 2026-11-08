//! Endpoint registers and USB SRAM of the STM32 USB device peripheral (usb_v4 with 32-bit USB SRAM: G0B1 / G0C1,
//! and H5, U0, U3, C0 parts with the same peripheral), computed from the [Device] at compile time.
//!
//! [Layout::new] gives every endpoint register its type and buffers and the initial value of both words of its
//! buffer descriptor table entry, so the driver's bus reset is a handful of stores. Endpoint number n uses
//! register n (`CHEPnR`, its EA field n); endpoint 0 is the control endpoint in register 0.
//!
//! What it checks, quoted from RM0444 Rev 6 (each a panic in `const` evaluation, so a build error):
//! - Eight registers: "Address offset: 0x0 + 0x8 * n, (n = 0 to 7)" (§37.7.1), so endpoint numbers 1 to 7.
//! - The buffer table and every packet buffer fit in the "Dedicated packet buffer memory SRAM of 2048 bytes"
//!   (Table 225).
//! - A double-buffered bulk endpoint takes a whole register: "the USB_CHEPnR registers used to implement
//!   double-buffered bulk endpoints are forced to be used as unidirectional ones ... In case it is required to have
//!   double-buffered bulk endpoints enabled both for reception and transmission, two USB_CHEPnR registers must be
//!   used" (§37.5.3).
//! - One transfer type per register (`UTYPE`), so an OUT and an IN endpoint on one number have the same type.
//! - An OUT buffer is exactly the endpoint's max packet size, and that size is encodable in BLSIZE / NUM_BLOCK:
//!   "If BLSIZE = 0 ... the allocated buffer size ranges from 2 to 62 bytes", "If BLSIZE = 1 ... from 32 to 1024
//!   bytes" (§37.7.2, Table 239). Equal, because a packet longer than the buffer is answered with a STALL and no
//!   interrupt (§37.5.2), never with the overrun a bigger buffer would hide.
//! - Buffers are word aligned: "Bits 1 and 0 must always be written as “00” since packet memory is word wide and
//!   all packet buffers must be word aligned" (§37.7.1); the table itself starts at 0, "aligned to an 8-byte
//!   boundary" (§37.5.2).

use crate::desc::{Device, Endpoints};
use crate::{Buffering, Direction, EndpointKind};

/// Endpoint registers `CHEP0R`..`CHEP7R`.
pub const REGISTERS: usize = 8;
/// Bytes of USB SRAM (RM0444 Table 225).
pub const SRAM_SIZE: usize = 2048;

/// `UTYPE` of an endpoint register.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum RegType {
    Bulk,
    Control,
    Interrupt,
}

impl RegType {
    /// The `UTYPE[1:0]` field value (RM0444 §37.6.7, CHEPnR bits 10:9: 00 bulk, 01 control, 10 iso, 11 interrupt).
    pub const fn utype(self) -> u32 {
        match self {
            RegType::Bulk => 0b00,
            RegType::Control => 0b01,
            RegType::Interrupt => 0b11,
        }
    }
}

/// How an endpoint register is used.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum RegMode {
    Unused,
    /// Single-buffered: an IN buffer in descriptor word 0 (`TXRXBD`) and / or an OUT buffer in word 1 (`RXTXBD`);
    /// a size of 0 means that direction is disabled.
    Single {
        out_size: u16,
        in_size: u16,
    },
    /// Double-buffered, one direction (`DBL_BUF` / `EPKIND` set): two buffers of `size` bytes, in words 0 and 1.
    Double {
        direction: Direction,
        size: u16,
    },
}

/// One endpoint register and its buffer descriptor table entry.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub struct Reg {
    pub mode: RegMode,
    pub reg_type: RegType,
    /// Byte offsets in USB SRAM of the buffers that words 0 and 1 describe (0: none).
    pub buffer: [u16; 2],
    /// Initial values of the entry's two words: `ADDR` and `COUNT` = 0 for a transmit buffer, `ADDR` and the
    /// allocated size (BLSIZE, NUM_BLOCK) for a receive buffer (RM0444 §37.7).
    pub bd: [u32; 2],
}

impl Reg {
    const UNUSED: Reg = Reg {
        mode: RegMode::Unused,
        reg_type: RegType::Bulk,
        buffer: [0; 2],
        bd: [0; 2],
    };

    pub const fn is_used(&self) -> bool {
        !matches!(self.mode, RegMode::Unused)
    }
}

/// The endpoint registers and USB SRAM of a device.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub struct Layout {
    pub regs: [Reg; REGISTERS],
    /// Registers in use: endpoint numbers 0..count.
    pub count: u8,
    /// Bytes of the buffer descriptor table at offset 0, 8 per register in use.
    pub table_len: u16,
    /// First byte of USB SRAM after the last packet buffer.
    pub end: u16,
}

impl Layout {
    /// The layout of `device`; panics (fails the build in `const`) where the peripheral can't serve it.
    pub const fn new(device: &Device) -> Layout {
        device.check();
        let mut regs = [Reg::UNUSED; REGISTERS];
        let mps0 = device.max_packet_size_0 as u16;
        regs[0] = Reg {
            mode: RegMode::Single {
                out_size: mps0,
                in_size: mps0,
            },
            reg_type: RegType::Control,
            ..Reg::UNUSED
        };

        let mut count = 1u8;
        let mut it = Endpoints::new(device);
        while let Some((_, ep)) = it.next() {
            let n = ep.address.number() as usize;
            assert!(
                n < REGISTERS,
                "endpoint numbers are 1 to 7: the peripheral has 8 endpoint registers"
            );
            if n as u8 + 1 > count {
                count = n as u8 + 1;
            }
            let reg_type = match ep.kind {
                EndpointKind::Bulk => RegType::Bulk,
                EndpointKind::Interrupt { .. } => RegType::Interrupt,
            };
            let dir = ep.address.direction();
            let reg = &mut regs[n];
            match ep.buffering {
                Buffering::Double => {
                    // the same endpoint again (an alternate setting) leaves it as it is, Device::check made sure
                    // it is the same one
                    match reg.mode {
                        RegMode::Unused => {}
                        RegMode::Double { direction, .. } if same_dir(direction, dir) => {}
                        _ => panic!(
                            "a double-buffered endpoint takes its whole register: no other endpoint on its number"
                        ),
                    }
                    reg.mode = RegMode::Double {
                        direction: dir,
                        size: ep.max_packet,
                    };
                    reg.reg_type = reg_type;
                }
                Buffering::Single => {
                    let (mut out_size, mut in_size) = match reg.mode {
                        RegMode::Unused => (0, 0),
                        RegMode::Single { out_size, in_size } => {
                            assert!(
                                matches!(
                                    (reg.reg_type, reg_type),
                                    (RegType::Bulk, RegType::Bulk)
                                        | (RegType::Interrupt, RegType::Interrupt)
                                ),
                                "an endpoint register has one type: OUT and IN of one number both bulk or both interrupt"
                            );
                            (out_size, in_size)
                        }
                        _ => panic!(
                            "a double-buffered endpoint takes its whole register: no other endpoint on its number"
                        ),
                    };
                    match dir {
                        Direction::Out => out_size = ep.max_packet,
                        Direction::In => in_size = ep.max_packet,
                    }
                    reg.mode = RegMode::Single { out_size, in_size };
                    reg.reg_type = reg_type;
                }
            }
        }

        let table_len = 8 * count as usize;
        let mut next = table_len;
        let mut n = 0;
        while n < count as usize {
            let reg = &mut regs[n];
            let (sizes, rx) = match reg.mode {
                RegMode::Unused => ([0, 0], [false, false]),
                RegMode::Single { out_size, in_size } => ([in_size, out_size], [false, true]),
                RegMode::Double {
                    direction: Direction::Out,
                    size,
                } => ([size, size], [true, true]),
                RegMode::Double {
                    direction: Direction::In,
                    size,
                } => ([size, size], [false, false]),
            };
            let mut w = 0;
            while w < 2 {
                if sizes[w] > 0 {
                    reg.buffer[w] = next as u16;
                    reg.bd[w] = next as u32 | if rx[w] { rx_size_field(sizes[w]) } else { 0 };
                    next += (sizes[w] as usize + 3) & !3;
                }
                w += 1;
            }
            n += 1;
        }
        assert!(
            next <= SRAM_SIZE,
            "the buffer table and packet buffers don't fit in the 2048 bytes of USB SRAM"
        );

        Layout {
            regs,
            count,
            table_len: table_len as u16,
            end: next as u16,
        }
    }

    /// Register `n`.
    pub const fn reg(&self, n: u8) -> &Reg {
        &self.regs[n as usize]
    }
}

const fn same_dir(a: Direction, b: Direction) -> bool {
    matches!(
        (a, b),
        (Direction::Out, Direction::Out) | (Direction::In, Direction::In)
    )
}

/// BLSIZE (bit 31) and NUM_BLOCK (bits 30:26) of a receive buffer of exactly `size` bytes (RM0444 Table 239):
/// 2-byte blocks, NUM_BLOCK 1 to 31, for 2 to 62 bytes; 32-byte blocks, NUM_BLOCK + 1 of them, for 32 to 1024.
/// Panics on a size neither can say exactly.
pub const fn rx_size_field(size: u16) -> u32 {
    if size >= 2 && size <= 62 && size.is_multiple_of(2) {
        ((size as u32) / 2) << 26
    } else if size >= 32 && size <= 1024 && size.is_multiple_of(32) {
        (1 << 31) | (((size as u32) / 32 - 1) << 26)
    } else {
        panic!(
            "an OUT buffer equals its max packet size, which BLSIZE / NUM_BLOCK can't encode: even up to 62, or a multiple of 32"
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Buffering::{Double, Single};
    use crate::desc::{Endpoint, Function, Interface, Item};

    #[test]
    fn rx_size_encoding() {
        assert_eq!(rx_size_field(2), 1 << 26);
        assert_eq!(rx_size_field(8), 4 << 26);
        assert_eq!(rx_size_field(62), 31 << 26);
        assert_eq!(rx_size_field(64), 0x8000_0000 | (1 << 26));
        assert_eq!(rx_size_field(32), 16 << 26); // 2-byte blocks win where both can say it
        assert_eq!(rx_size_field(96), 0x8000_0000 | (2 << 26));
        assert_eq!(rx_size_field(1024), 0x8000_0000 | (31 << 26));
    }

    #[test]
    #[should_panic(expected = "BLSIZE / NUM_BLOCK can't encode")]
    fn rx_size_odd() {
        rx_size_field(7);
    }

    const fn device(functions: &'static [Function]) -> Device {
        Device {
            functions,
            ..Device::new(1, 2)
        }
    }

    #[test]
    fn ep0_only() {
        const L: Layout = Layout::new(&device(&[]));
        assert_eq!((L.count, L.table_len, L.end), (1, 8, 8 + 64 + 64));
        assert_eq!(
            L.regs[0].mode,
            RegMode::Single {
                out_size: 64,
                in_size: 64
            }
        );
        assert_eq!(L.regs[0].reg_type, RegType::Control);
        assert_eq!(L.regs[0].buffer, [8, 72]);
        assert_eq!(L.regs[0].bd, [8, 72 | 0x8000_0000 | (1 << 26)]);
        assert!(!L.regs[1].is_used());
    }

    #[test]
    fn mixed_registers() {
        const F: &[Function] = &[Function {
            interfaces: &[Interface {
                items: &[
                    Item::Endpoint(Endpoint::bulk_out(1, 64, Double)),
                    Item::Endpoint(Endpoint::bulk_in(2, 64, Double)),
                    Item::Endpoint(Endpoint::bulk_out(3, 32, Single)),
                    Item::Endpoint(Endpoint::bulk_in(3, 64, Single)),
                    Item::Endpoint(Endpoint::interrupt_in(5, 10, 1)),
                ],
                ..Interface::vendor()
            }],
            ..Function::vendor()
        }];
        const L: Layout = Layout::new(&device(F));
        assert_eq!((L.count, L.table_len), (6, 48));
        let rx64 = 0x8000_0000 | (1 << 26);
        // EP0 64 + 64 from 48
        assert_eq!(L.regs[0].bd, [48, 112 | rx64]);
        // EP1 double OUT: both words receive buffers
        assert_eq!(
            L.regs[1].mode,
            RegMode::Double {
                direction: Direction::Out,
                size: 64
            }
        );
        assert_eq!(L.regs[1].bd, [176 | rx64, 240 | rx64]);
        // EP2 double IN: both words transmit buffers
        assert_eq!(L.regs[2].bd, [304, 368]);
        // EP3: IN 64 in word 0, OUT 32 in word 1 (BLSIZE 0, 16 blocks)
        assert_eq!(
            L.regs[3].mode,
            RegMode::Single {
                out_size: 32,
                in_size: 64
            }
        );
        assert_eq!(L.regs[3].bd, [432, 496 | (16 << 26)]);
        assert!(!L.regs[4].is_used());
        // EP5 interrupt IN of 10 bytes: buffer rounded to 12 for the next one's alignment
        assert_eq!(L.regs[5].reg_type, RegType::Interrupt);
        assert_eq!(L.regs[5].bd, [528, 0]);
        assert_eq!(L.end, 540);
    }

    #[test]
    #[should_panic(expected = "takes its whole register")]
    fn double_shares_register() {
        const F: &[Function] = &[Function {
            interfaces: &[Interface {
                items: &[
                    Item::Endpoint(Endpoint::bulk_out(1, 64, Double)),
                    Item::Endpoint(Endpoint::bulk_in(1, 64, Single)),
                ],
                ..Interface::vendor()
            }],
            ..Function::vendor()
        }];
        Layout::new(&device(F));
    }

    #[test]
    #[should_panic(expected = "8 endpoint registers")]
    fn endpoint_8() {
        const F: &[Function] = &[Function {
            interfaces: &[Interface {
                items: &[Item::Endpoint(Endpoint::bulk_in(8, 64, Single))],
                ..Interface::vendor()
            }],
            ..Function::vendor()
        }];
        Layout::new(&device(F));
    }

    #[test]
    #[should_panic(expected = "one type")]
    fn mixed_types_on_one_number() {
        const F: &[Function] = &[Function {
            interfaces: &[Interface {
                items: &[
                    Item::Endpoint(Endpoint::bulk_out(1, 64, Single)),
                    Item::Endpoint(Endpoint::interrupt_in(1, 8, 10)),
                ],
                ..Interface::vendor()
            }],
            ..Function::vendor()
        }];
        Layout::new(&device(F));
    }

    #[test]
    #[should_panic(expected = "can't encode")]
    fn odd_interrupt_out() {
        const F: &[Function] = &[Function {
            interfaces: &[Interface {
                items: &[Item::Endpoint(Endpoint::interrupt_out(1, 7, 10))],
                ..Interface::vendor()
            }],
            ..Function::vendor()
        }];
        Layout::new(&device(F));
    }

    #[test]
    fn worst_case_fits() {
        // the most a full-speed device can ask for: 64-byte EP0 and seven double-buffered 64-byte bulk endpoints,
        // 8 * 8 + 2 * 64 + 7 * 128 = 1088 bytes, so the 2048-byte check only matters to other packet sizes
        const EPS: &[Item] = &[
            Item::Endpoint(Endpoint::bulk_out(1, 64, Double)),
            Item::Endpoint(Endpoint::bulk_in(2, 64, Double)),
            Item::Endpoint(Endpoint::bulk_out(3, 64, Double)),
            Item::Endpoint(Endpoint::bulk_in(4, 64, Double)),
            Item::Endpoint(Endpoint::bulk_out(5, 64, Double)),
            Item::Endpoint(Endpoint::bulk_in(6, 64, Double)),
            Item::Endpoint(Endpoint::bulk_out(7, 64, Double)),
        ];
        const F: &[Function] = &[Function {
            interfaces: &[Interface {
                items: EPS,
                ..Interface::vendor()
            }],
            ..Function::vendor()
        }];
        const L: Layout = Layout::new(&device(F));
        assert_eq!((L.count, L.end), (8, 1088));
    }
}
