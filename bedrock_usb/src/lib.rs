//! A USB full-speed device stack for STM32 whose device is fixed at compile time.
//!
//! The firmware describes its device as a `const` [desc::Device]. `const fn`s turn it into the descriptor arrays the
//! control endpoint serves ([desc]) and into the endpoint registers and USB SRAM layout the driver programs
//! ([layout]); [descriptors!] names both and makes the build fail on a device the hardware can't serve. Nothing is
//! allocated, parsed or built at run time. [v4] drives the peripheral from that layout.
//!
//! [control] answers the host's requests on endpoint 0 from those tables, as a state machine the driver feeds with
//! what happened on EP0. [Usb] joins the two: [Usb::run] serves the bus and EP0 and keeps the bus status
//! ([v4::Status]) for the firmware.
//!
//! This module holds the vocabulary the other parts share: endpoint addresses and kinds, single or double
//! buffering, the setup packet and the standard request and descriptor codes.

#![no_std]

#[cfg(test)]
extern crate std;

pub mod control;
pub mod desc;
pub mod device;
pub mod layout;
mod macros;
#[doc(hidden)]
pub use macros::__byte;
pub mod v4;

pub use device::Usb;

/// Transfer direction, seen from the host as USB names it: OUT is host to device, IN device to host.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum Direction {
    Out,
    In,
}

/// An endpoint address as `bEndpointAddress` holds it: the number in bits 3:0, bit 7 set for IN.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub struct EndpointAddress(u8);

impl EndpointAddress {
    /// The address of endpoint `number` (0-15) in `direction`.
    pub const fn new(number: u8, direction: Direction) -> Self {
        assert!(number < 16, "endpoint numbers are 0 to 15");
        match direction {
            Direction::Out => EndpointAddress(number),
            Direction::In => EndpointAddress(number | 0x80),
        }
    }

    /// From the byte of a descriptor or of `wIndex`; the reserved bits 6:4 are dropped.
    pub const fn from_raw(raw: u8) -> Self {
        EndpointAddress(raw & 0x8f)
    }

    pub const fn raw(self) -> u8 {
        self.0
    }

    pub const fn number(self) -> u8 {
        self.0 & 0x0f
    }

    pub const fn direction(self) -> Direction {
        if self.0 & 0x80 != 0 {
            Direction::In
        } else {
            Direction::Out
        }
    }

    pub const fn is_in(self) -> bool {
        self.0 & 0x80 != 0
    }
}

/// Transfer type of a class endpoint. Control is endpoint 0's alone; isochronous is not in bedrock_usb 0.1.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum EndpointKind {
    Bulk,
    /// `interval` is `bInterval`, the polling period in frames (1 ms each at full speed), 1 to 255.
    Interrupt {
        interval: u8,
    },
}

impl EndpointKind {
    /// `bmAttributes` of the endpoint descriptor (bits 1:0, the transfer type).
    pub const fn attributes(self) -> u8 {
        match self {
            EndpointKind::Bulk => 0x02,
            EndpointKind::Interrupt { .. } => 0x03,
        }
    }

    /// `bInterval`: 0 for bulk (full speed ignores it), the polling period for interrupt.
    pub const fn interval(self) -> u8 {
        match self {
            EndpointKind::Bulk => 0,
            EndpointKind::Interrupt { interval } => interval,
        }
    }

    pub const fn is_bulk(self) -> bool {
        matches!(self, EndpointKind::Bulk)
    }
}

/// How the peripheral buffers an endpoint's packets, as a value in a const [desc::Endpoint].
///
/// `Single`: one packet buffer; the endpoint NAKs from the end of a transfer until the firmware has handled it.
/// `Double` (bulk only): two buffers, the peripheral uses one while the firmware works on the other; the endpoint
/// takes a whole endpoint register, both directions of it (RM0444 §37.5.3).
///
/// The same choice as a type is [buffer::Single] / [buffer::Double], for typed endpoints.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum Buffering {
    Single,
    Double,
}

/// [Buffering] as zero-sized types, so a typed endpoint (`BulkOut<'d, 1, buffer::Double>`) compiles only the path
/// it uses and can check against the descriptor in a `const { assert!(..) }`.
pub mod buffer {
    use crate::Buffering;

    /// One packet buffer.
    #[derive(Copy, Clone, Debug, Default)]
    pub struct Single;
    /// Two packet buffers, bulk only.
    #[derive(Copy, Clone, Debug, Default)]
    pub struct Double;

    /// Implemented by [Single] and [Double] only.
    pub trait Mode: sealed::Sealed + Copy + 'static {
        const BUFFERING: Buffering;
    }

    impl Mode for Single {
        const BUFFERING: Buffering = Buffering::Single;
    }
    impl Mode for Double {
        const BUFFERING: Buffering = Buffering::Double;
    }

    mod sealed {
        pub trait Sealed {}
        impl Sealed for super::Single {}
        impl Sealed for super::Double {}
    }
}

/// The 8-byte setup packet of a control transfer (USB 2.0 §9.3), fields in host order.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub struct Setup {
    pub request_type: u8,
    pub request: u8,
    pub value: u16,
    pub index: u16,
    pub length: u16,
}

/// Bits 6:5 of `bmRequestType`.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum RequestKind {
    Standard,
    Class,
    Vendor,
    Reserved,
}

/// Bits 4:0 of `bmRequestType`; 4-31 are reserved and read as `Other`.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum Recipient {
    Device,
    Interface,
    Endpoint,
    Other,
}

impl Setup {
    pub const fn parse(raw: &[u8; 8]) -> Self {
        Setup {
            request_type: raw[0],
            request: raw[1],
            value: u16::from_le_bytes([raw[2], raw[3]]),
            index: u16::from_le_bytes([raw[4], raw[5]]),
            length: u16::from_le_bytes([raw[6], raw[7]]),
        }
    }

    pub const fn to_bytes(&self) -> [u8; 8] {
        let v = self.value.to_le_bytes();
        let i = self.index.to_le_bytes();
        let l = self.length.to_le_bytes();
        [
            self.request_type,
            self.request,
            v[0],
            v[1],
            i[0],
            i[1],
            l[0],
            l[1],
        ]
    }

    /// The direction of the data stage (bit 7); a request without data stage has `Out`.
    pub const fn direction(&self) -> Direction {
        if self.request_type & 0x80 != 0 {
            Direction::In
        } else {
            Direction::Out
        }
    }

    pub const fn kind(&self) -> RequestKind {
        match (self.request_type >> 5) & 0x03 {
            0 => RequestKind::Standard,
            1 => RequestKind::Class,
            2 => RequestKind::Vendor,
            _ => RequestKind::Reserved,
        }
    }

    pub const fn recipient(&self) -> Recipient {
        match self.request_type & 0x1f {
            0 => Recipient::Device,
            1 => Recipient::Interface,
            2 => Recipient::Endpoint,
            _ => Recipient::Other,
        }
    }

    /// GET_DESCRIPTOR's descriptor type, the high byte of `wValue`.
    pub const fn descriptor_type(&self) -> u8 {
        (self.value >> 8) as u8
    }

    /// GET_DESCRIPTOR's descriptor index, the low byte of `wValue`.
    pub const fn descriptor_index(&self) -> u8 {
        self.value as u8
    }
}

/// Standard request codes, `bRequest` (USB 2.0 Table 9-4).
pub mod request {
    pub const GET_STATUS: u8 = 0;
    pub const CLEAR_FEATURE: u8 = 1;
    pub const SET_FEATURE: u8 = 3;
    pub const SET_ADDRESS: u8 = 5;
    pub const GET_DESCRIPTOR: u8 = 6;
    pub const SET_DESCRIPTOR: u8 = 7;
    pub const GET_CONFIGURATION: u8 = 8;
    pub const SET_CONFIGURATION: u8 = 9;
    pub const GET_INTERFACE: u8 = 10;
    pub const SET_INTERFACE: u8 = 11;
    pub const SYNCH_FRAME: u8 = 12;

    /// Feature selectors (USB 2.0 Table 9-6).
    pub const FEATURE_ENDPOINT_HALT: u16 = 0;
    pub const FEATURE_DEVICE_REMOTE_WAKEUP: u16 = 1;

    /// `wIndex` of the MS OS 2.0 vendor request that reads the descriptor set (MS OS 2.0 specification,
    /// MS_OS_20_DESCRIPTOR_INDEX); 8 is MS_OS_20_SET_ALT_ENUMERATION.
    pub const MS_OS_20_DESCRIPTOR_INDEX: u16 = 7;
    /// `wIndex` of WebUSB's GET_URL vendor request (WebUSB specification §9.3).
    pub const WEBUSB_GET_URL: u16 = 2;
}

/// Descriptor type codes, `bDescriptorType` (USB 2.0 Table 9-5, USB 3.2 for BOS).
pub mod descriptor_type {
    pub const DEVICE: u8 = 1;
    pub const CONFIGURATION: u8 = 2;
    pub const STRING: u8 = 3;
    pub const INTERFACE: u8 = 4;
    pub const ENDPOINT: u8 = 5;
    pub const DEVICE_QUALIFIER: u8 = 6;
    pub const OTHER_SPEED_CONFIGURATION: u8 = 7;
    pub const INTERFACE_ASSOCIATION: u8 = 11;
    pub const BOS: u8 = 15;
    pub const DEVICE_CAPABILITY: u8 = 16;
}

/// A device's descriptors and layout as a type, implemented by [descriptors!]: what the control machine, the
/// driver and typed endpoints are generic over.
pub trait Descriptors: 'static {
    const DEVICE: desc::Device;
    const DEVICE_DESC: [u8; 18];
    const CONFIG_DESC: &'static [u8];
    /// Empty on a USB 2.0 device.
    const BOS: &'static [u8];
    /// Empty without MS OS 2.0 descriptors.
    const MS_OS_20_SET: &'static [u8];
    const LAYOUT: layout::Layout;
}

/// The one configuration a bedrock_usb device has, its `bConfigurationValue`.
pub const CONFIGURATION_VALUE: u8 = 1;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn endpoint_address() {
        let a = EndpointAddress::new(2, Direction::In);
        assert_eq!(a.raw(), 0x82);
        assert_eq!(a.number(), 2);
        assert_eq!(a.direction(), Direction::In);
        assert_eq!(
            EndpointAddress::from_raw(0xf1),
            EndpointAddress::new(1, Direction::In)
        );
        assert_eq!(EndpointAddress::new(1, Direction::Out).raw(), 0x01);
    }

    #[test]
    fn setup_fields() {
        // GET_DESCRIPTOR(configuration 0), wLength 255
        let raw = [0x80, 0x06, 0x00, 0x02, 0x00, 0x00, 0xff, 0x00];
        let s = Setup::parse(&raw);
        assert_eq!(s.direction(), Direction::In);
        assert_eq!(s.kind(), RequestKind::Standard);
        assert_eq!(s.recipient(), Recipient::Device);
        assert_eq!(s.request, request::GET_DESCRIPTOR);
        assert_eq!(s.descriptor_type(), descriptor_type::CONFIGURATION);
        assert_eq!(s.descriptor_index(), 0);
        assert_eq!(s.length, 255);
        assert_eq!(s.to_bytes(), raw);
        // vendor request to an interface, OUT
        let v = Setup::parse(&[0x41, 0x01, 0, 0, 3, 0, 0, 0]);
        assert_eq!(
            (v.kind(), v.recipient(), v.direction()),
            (RequestKind::Vendor, Recipient::Interface, Direction::Out)
        );
    }
}
