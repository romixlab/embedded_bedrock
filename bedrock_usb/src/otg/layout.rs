//! The FIFO RAM of a Synopsys OTG core and the driver's OUT packet buffers, computed from the [Device] at compile
//! time.
//!
//! [Layout::new] gives every endpoint its transfer type, its max packet size at each speed, the receive FIFO's
//! depth, a transmit FIFO per IN endpoint (FIFO n for endpoint n), and a RAM buffer per OUT endpoint that the
//! interrupt handler empties the receive FIFO into. The driver's bus reset programs the FIFO registers from it.
//! The FIFOs are sized for the device's top speed (512-byte bulk packets on a high-speed capable device), so the
//! same layout serves a full-speed and a high-speed enumeration.
//!
//! What it checks, quoted from RM0468 Rev 3 (each a panic in `const` evaluation, so a build error):
//! - Endpoint numbers below the core's bidirectional endpoints, "Device bidirectional endpoints (including EP0)
//!   9" on the H72x (Table 531).
//! - The receive FIFO and every transmit FIFO fit in "a dedicated RAM of 4 Kbytes" (§62.2.1, Table 531: "Size of
//!   dedicated SRAM 4 Kbytes").
//! - Each transmit FIFO holds whole packets and at least 16 words: `OTG_DIEPTXFx` "INEPTXFD ... Minimum value is
//!   16" (§62.14.19), `OTG_DIEPTXF0` "TX0FD ... Minimum value is 16"; "the minimum RAM space required for each IN
//!   endpoint Transmit FIFO is the maximum packet size for that particular IN endpoint" (§62.11.3).
//!
//! The receive FIFO follows §62.11.3: "Device RxFIFO = (5 * number of control endpoints + 8) + ((largest USB
//! packet used / 4) + 1 for status information) + (2 * number of OUT endpoints) + 1 for Global NAK", with the
//! packet term twice ("Typically, two (largest packet size / 4) + 1 spaces are recommended so that when the
//! previous packet is being transferred to the CPU, the USB can receive the subsequent packet"), and
//! [RX_EXTRA_WORDS] more.

use crate::desc::{Device, Endpoints};
use crate::{Buffering, Direction, EndpointKind, Speed};

/// What a core instance has: its FIFO RAM and its endpoints.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub struct Core {
    /// FIFO RAM in 32-bit words.
    pub fifo_words: u16,
    /// Bidirectional endpoints, EP0 included.
    pub endpoints: u8,
}

impl Core {
    /// USB1 OTG_HS of the STM32H723 / H725 / H730 / H733 / H735 (RM0468 Rev 3 Table 531: "Device bidirectional
    /// endpoints (including EP0) 9", "Size of dedicated SRAM 4 Kbytes"): ULPI for an external high-speed PHY,
    /// or the embedded full-speed PHY.
    pub const H7_OTG_HS: Core = Core {
        fifo_words: 1024,
        endpoints: 9,
    };
}

/// Endpoints of the largest core: the layout's arrays.
pub const MAX_ENDPOINTS: usize = 9;

/// Receive FIFO words beyond §62.11.3's formula. embassy-stm32 (`usb/otg.rs`, from the `synopsys-usb-otg` crate)
/// adds 30: "This calculation doesn't correspond to one in a Reference Manual. In fact, the required number of
/// words is higher than indicated in RM. The following numbers are pessimistic and were figured out
/// empirically." bedrock_usb keeps them until a hardware test (B135) says otherwise.
pub const RX_EXTRA_WORDS: u16 = 30;

/// Minimum depth of a transmit FIFO in words (§62.14.19).
pub const TX_MIN_WORDS: u16 = 16;

/// Transfer type of one direction of an endpoint.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum EpType {
    Unused,
    Control,
    Bulk,
    Interrupt,
}

impl EpType {
    /// EPTYP of `OTG_DxEPCTLx`.
    pub const fn eptyp(self) -> u32 {
        match self {
            EpType::Bulk => super::regs::ctl::EPTYP_BULK,
            EpType::Interrupt => super::regs::ctl::EPTYP_INTERRUPT,
            EpType::Unused | EpType::Control => 0,
        }
    }
}

/// One direction of one endpoint.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub struct Dir {
    pub ep_type: EpType,
    /// Max packet size at full speed and at high speed ([crate::desc::Endpoint::max_packet_at]).
    pub max_packet: [u16; 2],
    /// Packets it buffers: 1 for `Single`, 2 for `Double`.
    pub packets: u8,
    /// OUT: word offset of its first packet buffer in the driver's RAM; IN: start word of its transmit FIFO.
    pub start: u16,
    /// OUT: words of one packet buffer; IN: depth of its transmit FIFO in words.
    pub words: u16,
}

impl Dir {
    const UNUSED: Dir = Dir {
        ep_type: EpType::Unused,
        max_packet: [0; 2],
        packets: 0,
        start: 0,
        words: 0,
    };

    pub const fn is_used(&self) -> bool {
        !matches!(self.ep_type, EpType::Unused)
    }

    pub const fn max_packet_at(&self, speed: Speed) -> u16 {
        match speed {
            Speed::Full => self.max_packet[0],
            Speed::High => self.max_packet[1],
        }
    }
}

/// Both directions of endpoint n.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub struct Ep {
    pub out: Dir,
    pub inp: Dir,
}

/// The FIFO RAM and OUT buffers of a device on a core.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub struct Layout {
    pub eps: [Ep; MAX_ENDPOINTS],
    /// Endpoint numbers in use: 0..count.
    pub count: u8,
    /// Depth of the receive FIFO in words; it starts at word 0.
    pub rx_words: u16,
    /// First FIFO word after the last transmit FIFO.
    pub fifo_end: u16,
    /// Words of RAM the OUT packet buffers take, EP0's included ([super::State]'s `W`).
    pub out_words: u16,
    pub core: Core,
}

const fn words(bytes: u16) -> u16 {
    bytes.div_ceil(4)
}

impl Layout {
    /// The layout of `device` on `core`; panics (fails the build in `const`) where the core can't serve it.
    pub const fn new(device: &Device, core: &Core) -> Layout {
        device.check();
        assert!(
            core.endpoints as usize <= MAX_ENDPOINTS,
            "a core has at most 9 endpoints"
        );
        let top = if device.high_speed {
            Speed::High
        } else {
            Speed::Full
        };
        let mut eps = [Ep {
            out: Dir::UNUSED,
            inp: Dir::UNUSED,
        }; MAX_ENDPOINTS];
        let mps0 = device.max_packet_size_0 as u16;
        let ep0 = Dir {
            ep_type: EpType::Control,
            max_packet: [mps0, mps0],
            packets: 1,
            start: 0,
            words: 0,
        };
        eps[0] = Ep { out: ep0, inp: ep0 };

        let mut count = 1u8;
        let mut it = Endpoints::new(device);
        while let Some((_, ep)) = it.next() {
            let n = ep.address.number() as usize;
            assert!(
                n < core.endpoints as usize,
                "endpoint number past the core's endpoints (8 on the H7's OTG_HS)"
            );
            if n as u8 + 1 > count {
                count = n as u8 + 1;
            }
            let dir = Dir {
                ep_type: match ep.kind {
                    EndpointKind::Bulk => EpType::Bulk,
                    EndpointKind::Interrupt { .. } => EpType::Interrupt,
                },
                max_packet: [ep.max_packet_at(Speed::Full), ep.max_packet_at(Speed::High)],
                packets: match ep.buffering {
                    Buffering::Single => 1,
                    Buffering::Double => 2,
                },
                start: 0,
                words: 0,
            };
            // the same endpoint again in another alternate setting: Device::check made sure it is the same one
            match ep.address.direction() {
                Direction::Out => eps[n].out = dir,
                Direction::In => eps[n].inp = dir,
            }
        }

        // OUT buffers in RAM: EP0 first, then by number
        let mut out_words = 0u16;
        let mut largest = mps0;
        let mut outs = 0u16;
        let mut n = 0;
        while n < count as usize {
            let d = &mut eps[n].out;
            if d.is_used() {
                let mps = d.max_packet_at(top);
                if mps > largest {
                    largest = mps;
                }
                outs += 1;
                d.start = out_words;
                d.words = words(mps);
                out_words += d.words * d.packets as u16;
            }
            n += 1;
        }

        // §62.11.3 with two packets, and the extra words
        let rx_words = (5 + 8) + 2 * (words(largest) + 1) + 2 * outs + 1 + RX_EXTRA_WORDS;
        let mut next = rx_words;
        let mut n = 0;
        while n < count as usize {
            let d = &mut eps[n].inp;
            if d.is_used() {
                let mut depth = words(d.max_packet_at(top)) * d.packets as u16;
                if depth < TX_MIN_WORDS {
                    depth = TX_MIN_WORDS;
                }
                d.start = next;
                d.words = depth;
                next += depth;
            }
            n += 1;
        }
        assert!(
            next <= core.fifo_words,
            "the receive FIFO and the transmit FIFOs don't fit in the core's FIFO RAM (4 KiB on the H7's OTG_HS)"
        );
        Layout {
            eps,
            count,
            rx_words,
            fifo_end: next,
            out_words,
            core: *core,
        }
    }

    /// Endpoint `n`'s `direction`, unused past the layout.
    pub const fn dir(&self, n: usize, direction: Direction) -> &Dir {
        match direction {
            Direction::Out => &self.eps[n].out,
            Direction::In => &self.eps[n].inp,
        }
    }

    /// Whether endpoint `n` has `direction` with `kind` and `packets` buffered, for a typed endpoint's
    /// `const { assert!(..) }`.
    pub const fn has(
        &self,
        n: u8,
        direction: Direction,
        kind: EpType,
        buffering: Buffering,
    ) -> bool {
        if n == 0 || n >= self.count {
            return false;
        }
        let d = self.dir(n as usize, direction);
        let packets = match buffering {
            Buffering::Single => 1,
            Buffering::Double => 2,
        };
        d.packets == packets
            && matches!(
                (d.ep_type, kind),
                (EpType::Bulk, EpType::Bulk) | (EpType::Interrupt, EpType::Interrupt)
            )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Buffering::{Double, Single};
    use crate::desc::{Endpoint, Function, Interface, Item};

    const HS: Device = Device {
        high_speed: true,
        functions: &[Function {
            interfaces: &[Interface {
                items: &[
                    Item::Endpoint(Endpoint::bulk_out(1, 64, Double)),
                    Item::Endpoint(Endpoint::bulk_in(1, 64, Double)),
                    Item::Endpoint(Endpoint::interrupt_in(3, 8, 1)),
                ],
                ..Interface::vendor()
            }],
            ..Function::vendor()
        }],
        ..Device::new(0xc0de, 0xcafe)
    };

    #[test]
    fn high_speed_bulk_pair() {
        const L: Layout = Layout::new(&HS, &Core::H7_OTG_HS);
        assert_eq!(L.count, 4);
        // EP0 OUT 16 words, EP1 OUT two 512-byte packets
        assert_eq!((L.eps[0].out.start, L.eps[0].out.words), (0, 16));
        assert_eq!((L.eps[1].out.start, L.eps[1].out.words), (16, 128));
        assert_eq!(L.out_words, 16 + 256);
        // (5 + 8) + 2 * (128 + 1) + 2 * 2 OUT endpoints + 1 + 30
        assert_eq!(L.rx_words, 13 + 258 + 4 + 1 + 30);
        // transmit FIFOs after it: EP0 16, EP1 two packets, EP3 the 16-word minimum
        assert_eq!((L.eps[0].inp.start, L.eps[0].inp.words), (L.rx_words, 16));
        assert_eq!(
            (L.eps[1].inp.start, L.eps[1].inp.words),
            (L.rx_words + 16, 256)
        );
        assert_eq!(L.eps[1].inp.max_packet, [64, 512]);
        assert_eq!(L.eps[3].inp.ep_type, EpType::Interrupt);
        assert_eq!(L.eps[3].inp.max_packet, [8, 8]);
        assert_eq!(L.eps[3].inp.words, 16);
        assert_eq!(L.fifo_end, L.rx_words + 16 + 256 + 16);
        assert!(!L.eps[2].out.is_used() && !L.eps[2].inp.is_used());
        assert!(L.has(1, Direction::Out, EpType::Bulk, Double));
        assert!(!L.has(1, Direction::Out, EpType::Bulk, Single));
        assert!(L.has(3, Direction::In, EpType::Interrupt, Single));
        assert!(!L.has(0, Direction::In, EpType::Bulk, Single));
    }

    #[test]
    fn full_speed_only_is_sized_for_64() {
        const L: Layout = Layout::new(
            &Device {
                high_speed: false,
                ..HS
            },
            &Core::H7_OTG_HS,
        );
        assert_eq!(L.eps[1].inp.words, 32);
        assert_eq!(L.eps[1].out.words, 16);
        assert_eq!(L.rx_words, 13 + 2 * 17 + 4 + 1 + 30);
    }
}
