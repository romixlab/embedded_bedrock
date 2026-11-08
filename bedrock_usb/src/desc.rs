//! The device as a `const` value and its descriptors as `const fn` output.
//!
//! A [Device] lists its functions, a function its interfaces, an interface its endpoints and class-specific
//! descriptors, all in `&'static` slices. Every descriptor it implies is encoded by a pair of `const fn`s: one that
//! gives the length (`config_len`) and one that writes `[u8; N]` with that length (`config_descriptor`), so the
//! arrays live in flash with exact sizes:
//!
//! ```
//! use bedrock_usb::desc::{Device, Function, Interface, Item, Endpoint, Str};
//! use bedrock_usb::Buffering::Single;
//!
//! const DEVICE: Device = Device {
//!     functions: &[Function {
//!         interfaces: &[Interface {
//!             string: Str::Const("bulk loopback"),
//!             items: &[
//!                 Item::Endpoint(Endpoint::bulk_out(1, 64, Single)),
//!                 Item::Endpoint(Endpoint::bulk_in(1, 64, Single)),
//!             ],
//!             ..Interface::vendor()
//!         }],
//!         ..Function::vendor()
//!     }],
//!     ..Device::new(0xc0de, 0xcafe)
//! };
//! const CONFIG: [u8; DEVICE.config_len()] = DEVICE.config_descriptor();
//! assert_eq!(CONFIG.len(), 9 + 8 + 9 + 7 + 7);
//! ```
//!
//! A device the encoding can't express (a descriptor over 255 bytes, a bulk endpoint of 100 bytes, two endpoints
//! on one address, an MS OS feature without the MS OS descriptor set) panics in `const` evaluation, which fails the
//! build; [Device::check] runs every such check at once, [crate::descriptors!] calls it. The checks here are the
//! USB ones; what one peripheral can do (eight endpoint registers, 2048 bytes of USB SRAM) is in [crate::layout].
//!
//! The byte layouts are those of USB 2.0 chapter 9, the IAD ECN (bInterfaceCount and the 0xEF / 0x02 / 0x01
//! device class), USB 3.2 §9.6.2 for BOS, the Microsoft OS 2.0 Descriptors Specification and WebUSB §4. Indices
//! and order follow embassy-usb's `Builder` where both have a choice, so a firmware that moves from embassy-usb
//! keeps its descriptors (the golden test in `tests/donguru.rs`).

use crate::{Buffering, Direction, EndpointAddress, EndpointKind, Speed, descriptor_type};

/// String index of [Device::manufacturer], fixed whether or not the device has one (as in embassy-usb).
pub const STRING_MANUFACTURER: u8 = 1;
/// String index of [Device::product].
pub const STRING_PRODUCT: u8 = 2;
/// String index of [Device::serial].
pub const STRING_SERIAL: u8 = 3;
/// Interface strings take the indices from here, in the order of the interfaces that have one.
pub const STRING_FIRST_INTERFACE: u8 = 4;
/// The one language of the string descriptors, English (United States).
pub const LANG_ID_EN_US: u16 = 0x0409;

/// `dwWindowsVersion` of Windows 8.1, the oldest the MS OS 2.0 descriptors work on.
pub const WINDOWS_8_1: u32 = 0x0603_0000;
/// `dwWindowsVersion` of Windows 10.
pub const WINDOWS_10: u32 = 0x0A00_0000;

/// A string descriptor's content.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum Str {
    /// No string: the index field is 0.
    None,
    /// Known at build time.
    Const(&'static str),
    /// Known only at run time (a serial number from the chip's UID, a label from the settings): the firmware
    /// serves it, the descriptor only reserves its index.
    Runtime,
}

impl Str {
    pub const fn is_some(&self) -> bool {
        !matches!(self, Str::None)
    }
}

/// `bDeviceClass` / `bDeviceSubClass` / `bDeviceProtocol`, and whether functions get an IAD.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum DeviceClass {
    /// 0xEF / 0x02 / 0x01 and an interface association descriptor before each function: what Windows needs to
    /// bind a driver per function of a composite device (IAD ECN).
    Composite,
    /// 0x00 / 0x00 / 0x00, each interface names its class; no IADs.
    PerInterface,
    /// A device class of its own; no IADs.
    Custom {
        class: u8,
        subclass: u8,
        protocol: u8,
    },
}

impl DeviceClass {
    pub const fn triple(self) -> (u8, u8, u8) {
        match self {
            DeviceClass::Composite => (0xEF, 0x02, 0x01),
            DeviceClass::PerInterface => (0, 0, 0),
            DeviceClass::Custom {
                class,
                subclass,
                protocol,
            } => (class, subclass, protocol),
        }
    }
}

/// The whole device: what the device, configuration, BOS and MS OS 2.0 descriptors say.
///
/// Start from [Device::new] and change fields with `..Device::new(vid, pid)`. A device has one configuration
/// ([crate::CONFIGURATION_VALUE]) and no isochronous endpoints. It runs at full speed only, unless [Device::high_speed]
/// says it can run at high speed too (the Synopsys OTG core behind a high-speed PHY): then it has a second
/// configuration descriptor for high speed and a device qualifier (USB 2.0 §9.6.2, §9.6.4).
#[derive(Copy, Clone, Debug)]
pub struct Device {
    pub vid: u16,
    pub pid: u16,
    /// `bcdDevice`, the device's release number.
    pub bcd_device: u16,
    /// `bcdUSB`: 0x0210 lets the host read the BOS descriptor (needed for [MsOs20] and [WebUsb]), 0x0200 has none.
    pub usb_version: u16,
    pub class: DeviceClass,
    /// `bMaxPacketSize0`: 8, 16, 32 or 64.
    pub max_packet_size_0: u8,
    pub manufacturer: Str,
    pub product: Str,
    pub serial: Str,
    pub self_powered: bool,
    pub remote_wakeup: bool,
    /// Current drawn from VBUS, in mA, at most 500; `bMaxPower` holds half of it.
    pub max_power_ma: u16,
    /// Microsoft OS 2.0 descriptors: a platform capability in the BOS and the descriptor set the vendor request reads.
    pub ms_os_20: Option<MsOs20>,
    /// WebUSB platform capability in the BOS.
    pub webusb: Option<WebUsb>,
    /// High-speed capable: [Device::config_descriptor_hs] and [Device::device_qualifier] exist, each endpoint's
    /// high-speed values are [Endpoint::max_packet_at] and [Endpoint::interval_at]. EP0 is then 64 bytes, the only
    /// size high speed has (USB 2.0 §5.5.3: "only 64-byte maximum data payload size for high-speed control
    /// endpoints").
    pub high_speed: bool,
    pub functions: &'static [Function],
}

impl Device {
    /// The defaults of embassy-usb's `Config::new`: USB 2.1, composite with IADs, 64-byte EP0, release 0x0010, bus
    /// powered at 100 mA, no strings, no functions.
    pub const fn new(vid: u16, pid: u16) -> Self {
        Device {
            vid,
            pid,
            bcd_device: 0x0010,
            usb_version: 0x0210,
            class: DeviceClass::Composite,
            max_packet_size_0: 64,
            manufacturer: Str::None,
            product: Str::None,
            serial: Str::None,
            self_powered: false,
            remote_wakeup: false,
            max_power_ma: 100,
            ms_os_20: None,
            webusb: None,
            high_speed: false,
            functions: &[],
        }
    }
}

/// A function: one IAD (on a [DeviceClass::Composite] device) and the interfaces it groups.
#[derive(Copy, Clone, Debug)]
pub struct Function {
    pub class: u8,
    pub subclass: u8,
    pub protocol: u8,
    /// Its interfaces with their alternate settings: an entry with `alt_setting` 0 starts the next interface
    /// number, an entry with `alt_setting` n > 0 is setting n of the interface before it.
    pub interfaces: &'static [Interface],
    /// Function-level MS OS 2.0 features: a function subset in the descriptor set when not empty.
    pub ms_os: &'static [MsOsFeature],
}

impl Function {
    /// A vendor-specific function (0xFF / 0x00 / 0x00) without interfaces, to fill in with `..Function::vendor()`.
    pub const fn vendor() -> Self {
        Function {
            class: 0xFF,
            subclass: 0,
            protocol: 0,
            interfaces: &[],
            ms_os: &[],
        }
    }
}

/// One interface descriptor (one alternate setting) and what follows it.
#[derive(Copy, Clone, Debug)]
pub struct Interface {
    pub alt_setting: u8,
    pub class: u8,
    pub subclass: u8,
    pub protocol: u8,
    pub string: Str,
    /// Endpoints and class-specific descriptors, written in this order after the interface descriptor.
    pub items: &'static [Item],
}

impl Interface {
    /// A vendor-specific interface (0xFF / 0x00 / 0x00), alternate setting 0, no string, nothing in it.
    pub const fn vendor() -> Self {
        Interface {
            alt_setting: 0,
            class: 0xFF,
            subclass: 0,
            protocol: 0,
            string: Str::None,
            items: &[],
        }
    }

    /// `bNumEndpoints`.
    pub const fn endpoint_count(&self) -> u8 {
        let mut n = 0;
        let mut i = 0;
        while i < self.items.len() {
            if let Item::Endpoint(_) = self.items[i] {
                n += 1;
            }
            i += 1;
        }
        n
    }
}

/// What follows an interface descriptor.
#[derive(Copy, Clone, Debug)]
pub enum Item {
    Endpoint(Endpoint),
    /// A class-specific (or vendor-specific) descriptor: `bLength` and `bDescriptorType` = `kind` are added in
    /// front of `data` (at most 253 bytes).
    Class {
        kind: u8,
        data: &'static [u8],
    },
}

/// A class endpoint (not endpoint 0).
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct Endpoint {
    pub address: EndpointAddress,
    pub kind: EndpointKind,
    /// `wMaxPacketSize` at full speed: 8, 16, 32 or 64 for bulk, 1 to 64 for interrupt (USB 2.0 §5.7.3, §5.8.3).
    /// The high-speed value follows from it, [Endpoint::max_packet_at].
    pub max_packet: u16,
    pub buffering: Buffering,
}

impl Endpoint {
    pub const fn bulk_out(number: u8, max_packet: u16, buffering: Buffering) -> Self {
        Self::bulk(
            EndpointAddress::new(number, Direction::Out),
            max_packet,
            buffering,
        )
    }

    pub const fn bulk_in(number: u8, max_packet: u16, buffering: Buffering) -> Self {
        Self::bulk(
            EndpointAddress::new(number, Direction::In),
            max_packet,
            buffering,
        )
    }

    pub const fn bulk(address: EndpointAddress, max_packet: u16, buffering: Buffering) -> Self {
        Endpoint {
            address,
            kind: EndpointKind::Bulk,
            max_packet,
            buffering,
        }
    }

    /// An interrupt IN endpoint polled every `interval` ms; single-buffered.
    pub const fn interrupt_in(number: u8, max_packet: u16, interval: u8) -> Self {
        Endpoint {
            address: EndpointAddress::new(number, Direction::In),
            kind: EndpointKind::Interrupt { interval },
            max_packet,
            buffering: Buffering::Single,
        }
    }

    /// An interrupt OUT endpoint polled every `interval` ms; single-buffered.
    pub const fn interrupt_out(number: u8, max_packet: u16, interval: u8) -> Self {
        Endpoint {
            address: EndpointAddress::new(number, Direction::Out),
            kind: EndpointKind::Interrupt { interval },
            max_packet,
            buffering: Buffering::Single,
        }
    }

    /// `wMaxPacketSize` at `speed`. High speed: 512 for bulk, the only size it has (USB 2.0 §5.8.3: "The USB
    /// defines the allowable maximum bulk data payload sizes to be only 8, 16, 32, or 64 bytes for full-speed
    /// endpoints and 512 bytes for high-speed endpoints"); an interrupt endpoint keeps its full-speed size, which
    /// high speed allows ("up to 1024 bytes", §5.7.3).
    pub const fn max_packet_at(&self, speed: Speed) -> u16 {
        match (speed, self.kind) {
            (Speed::High, EndpointKind::Bulk) => 512,
            _ => self.max_packet,
        }
    }

    /// `bInterval` at `speed`. Bulk: 0 (high speed: bulk OUT "never NAKs" as a NAK rate, §9.6.6). Interrupt at
    /// full speed: the period in ms. At high speed "the bInterval value is used as the exponent for a 2^(bInterval-1)
    /// value ... This value must be from 1 to 16" in 125 µs microframes (§9.6.6): the largest power of two of
    /// microframes not longer than the period asked for, so the host polls at least as often as at full speed.
    pub const fn interval_at(&self, speed: Speed) -> u8 {
        match (speed, self.kind) {
            (Speed::High, EndpointKind::Interrupt { interval }) => {
                let microframes = 8 * interval as u32;
                let exponent = 31 - microframes.leading_zeros();
                if exponent >= 15 {
                    16
                } else {
                    exponent as u8 + 1
                }
            }
            _ => self.kind.interval(),
        }
    }

    const fn same(&self, other: &Endpoint) -> bool {
        self.address.raw() == other.address.raw()
            && self.kind.attributes() == other.kind.attributes()
            && self.kind.interval() == other.kind.interval()
            && self.max_packet == other.max_packet
            && matches!(
                (self.buffering, other.buffering),
                (Buffering::Single, Buffering::Single) | (Buffering::Double, Buffering::Double)
            )
    }
}

/// The Microsoft OS 2.0 descriptor set and how the host reads it.
#[derive(Copy, Clone, Debug)]
pub struct MsOs20 {
    /// `bMS_VendorCode`: the `bRequest` of the vendor request that reads the set (`wIndex` 7).
    pub vendor_code: u8,
    /// `dwWindowsVersion`, at least [WINDOWS_8_1].
    pub windows_version: u32,
    /// Device-level features, right after the set header.
    pub device: &'static [MsOsFeature],
}

/// A feature descriptor of the MS OS 2.0 descriptor set.
#[derive(Copy, Clone, Debug)]
pub enum MsOsFeature {
    /// Compatible ID, e.g. `"WINUSB"` and `""`; each at most 8 ASCII characters, padded with zeros.
    CompatibleId {
        id: &'static str,
        sub_id: &'static str,
    },
    /// A registry property, e.g. `DeviceInterfaceGUIDs` as [RegValue::MultiSz].
    Registry { name: &'static str, value: RegValue },
}

/// The value of a registry property and its `wPropertyDataType`.
#[derive(Copy, Clone, Debug)]
pub enum RegValue {
    Sz(&'static str),
    ExpandSz(&'static str),
    Binary(&'static [u8]),
    DwordLe(u32),
    DwordBe(u32),
    Link(&'static str),
    MultiSz(&'static [&'static str]),
}

/// The WebUSB platform capability.
#[derive(Copy, Clone, Debug)]
pub struct WebUsb {
    /// `bVendorCode`: the `bRequest` of WebUSB's vendor requests (GET_URL).
    pub vendor_code: u8,
    /// `iLandingPage`, the URL index GET_URL serves; 0 for none.
    pub landing_page: u8,
}

/// Microsoft OS 2.0 platform capability UUID {D8DD60DF-4589-4CC7-9CD2-659D9E648A9F}, in descriptor byte order.
pub const MS_OS_20_UUID: [u8; 16] = [
    0xdf, 0x60, 0xdd, 0xd8, 0x89, 0x45, 0xc7, 0x4c, 0x9c, 0xd2, 0x65, 0x9d, 0x9e, 0x64, 0x8a, 0x9f,
];
/// WebUSB platform capability UUID {3408B638-09A9-47A0-8BFD-A0768815B665}, in descriptor byte order.
pub const WEBUSB_UUID: [u8; 16] = [
    0x38, 0xb6, 0x08, 0x34, 0xa9, 0x09, 0xa0, 0x47, 0x8b, 0xfd, 0xa0, 0x76, 0x88, 0x15, 0xb6, 0x65,
];

/// Writes bytes into `[u8; N]`, or with `N` = 0 only counts them: the length and the content of a descriptor come
/// from the same code.
pub(crate) struct Writer<const N: usize> {
    buf: [u8; N],
    pos: usize,
}

impl<const N: usize> Writer<N> {
    pub(crate) const fn new() -> Self {
        Writer {
            buf: [0; N],
            pos: 0,
        }
    }

    pub(crate) const fn pos(&self) -> usize {
        self.pos
    }

    pub(crate) const fn u8(&mut self, b: u8) {
        if self.pos < N {
            self.buf[self.pos] = b;
        }
        self.pos += 1;
    }

    pub(crate) const fn u16(&mut self, v: u16) {
        self.u8(v as u8);
        self.u8((v >> 8) as u8);
    }

    pub(crate) const fn u32(&mut self, v: u32) {
        self.u16(v as u16);
        self.u16((v >> 16) as u16);
    }

    pub(crate) const fn bytes(&mut self, b: &[u8]) {
        let mut i = 0;
        while i < b.len() {
            self.u8(b[i]);
            i += 1;
        }
    }

    /// `s` as UTF-16LE, without terminator.
    pub(crate) const fn utf16(&mut self, s: &str) {
        let b = s.as_bytes();
        let mut i = 0;
        while i < b.len() {
            let (cp, n) = decode_utf8(b, i);
            if cp >= 0x1_0000 {
                let v = cp - 0x1_0000;
                self.u16(0xD800 | (v >> 10) as u16);
                self.u16(0xDC00 | (v & 0x3ff) as u16);
            } else {
                self.u16(cp as u16);
            }
            i += n;
        }
    }

    /// `s` as UTF-16LE with a 0 terminator.
    pub(crate) const fn utf16z(&mut self, s: &str) {
        self.utf16(s);
        self.u16(0);
    }

    /// Overwrites the two bytes at `at` (written before as a placeholder).
    pub(crate) const fn patch_u16(&mut self, at: usize, v: u16) {
        if at + 1 < N {
            self.buf[at] = v as u8;
            self.buf[at + 1] = (v >> 8) as u8;
        }
    }

    pub(crate) const fn patch_u8(&mut self, at: usize, v: u8) {
        if at < N {
            self.buf[at] = v;
        }
    }

    pub(crate) const fn finish(self) -> [u8; N] {
        assert!(
            self.pos == N,
            "descriptor array length differs from its *_len()"
        );
        self.buf
    }
}

/// The code point at `b[i]` and its length in bytes; `b` is valid UTF-8 (it comes from a `&str`).
const fn decode_utf8(b: &[u8], i: usize) -> (u32, usize) {
    let c0 = b[i] as u32;
    if c0 < 0x80 {
        (c0, 1)
    } else if c0 < 0xE0 {
        (((c0 & 0x1f) << 6) | (b[i + 1] as u32 & 0x3f), 2)
    } else if c0 < 0xF0 {
        (
            ((c0 & 0x0f) << 12) | ((b[i + 1] as u32 & 0x3f) << 6) | (b[i + 2] as u32 & 0x3f),
            3,
        )
    } else {
        let cp = ((c0 & 0x07) << 18)
            | ((b[i + 1] as u32 & 0x3f) << 12)
            | ((b[i + 2] as u32 & 0x3f) << 6)
            | (b[i + 3] as u32 & 0x3f);
        (cp, 4)
    }
}

/// UTF-16 code units of `s`.
pub const fn utf16_len(s: &str) -> usize {
    let mut w = Writer::<0>::new();
    w.utf16(s);
    w.pos() / 2
}

/// Length of the string descriptor of `s`: 2 + 2 per UTF-16 code unit.
pub const fn string_len(s: &str) -> usize {
    2 + 2 * utf16_len(s)
}

/// The string descriptor of `s`; at most 126 UTF-16 code units, so `bLength` fits.
pub const fn string_descriptor<const N: usize>(s: &str) -> [u8; N] {
    let mut w = Writer::<N>::new();
    write_string(&mut w, s);
    w.finish()
}

const fn write_string<const N: usize>(w: &mut Writer<N>, s: &str) {
    let len = string_len(s);
    assert!(
        len <= 254,
        "a string descriptor holds at most 126 UTF-16 code units"
    );
    w.u8(len as u8);
    w.u8(descriptor_type::STRING);
    w.utf16(s);
}

/// String descriptor 0: the one language ID, [LANG_ID_EN_US].
pub const STRING0: [u8; 4] = [
    4,
    descriptor_type::STRING,
    LANG_ID_EN_US as u8,
    (LANG_ID_EN_US >> 8) as u8,
];

impl Device {
    /// The 18-byte device descriptor.
    pub const fn device_descriptor(&self) -> [u8; 18] {
        let (class, subclass, protocol) = self.class.triple();
        let v = self.usb_version.to_le_bytes();
        let vid = self.vid.to_le_bytes();
        let pid = self.pid.to_le_bytes();
        let rel = self.bcd_device.to_le_bytes();
        [
            18,
            descriptor_type::DEVICE,
            v[0],
            v[1],
            class,
            subclass,
            protocol,
            self.max_packet_size_0,
            vid[0],
            vid[1],
            pid[0],
            pid[1],
            rel[0],
            rel[1],
            if self.manufacturer.is_some() {
                STRING_MANUFACTURER
            } else {
                0
            },
            if self.product.is_some() {
                STRING_PRODUCT
            } else {
                0
            },
            if self.serial.is_some() {
                STRING_SERIAL
            } else {
                0
            },
            1, // bNumConfigurations
        ]
    }

    /// `bNumInterfaces`: the interface numbers, alternate settings not counted.
    pub const fn interface_count(&self) -> u8 {
        let mut n = 0;
        let mut f = 0;
        while f < self.functions.len() {
            n += function_interface_count(&self.functions[f]);
            f += 1;
        }
        n
    }

    /// Number of alternate settings of interface `number`; 0 for an interface the device doesn't have.
    pub const fn alt_settings(&self, number: u8) -> u8 {
        let mut it = Interfaces::new(self);
        let mut n = 0;
        while let Some(i) = it.next() {
            if i.number == number {
                n += 1;
            }
        }
        n
    }

    /// What the string descriptor `index` holds; `None` for an index the device doesn't use (0, the language
    /// IDs, is [STRING0]).
    pub const fn string(&self, index: u8) -> Option<Str> {
        let s = match index {
            0 => return None,
            STRING_MANUFACTURER => self.manufacturer,
            STRING_PRODUCT => self.product,
            STRING_SERIAL => self.serial,
            _ => {
                let mut it = Interfaces::new(self);
                while let Some(i) = it.next() {
                    if i.string_index == index {
                        return Some(i.interface.string);
                    }
                }
                Str::None
            }
        };
        if s.is_some() { Some(s) } else { None }
    }

    /// The endpoint at `address`, if any interface has it.
    pub const fn endpoint(&self, address: EndpointAddress) -> Option<Endpoint> {
        let mut it = Endpoints::new(self);
        while let Some((_, ep)) = it.next() {
            if ep.address.raw() == address.raw() {
                return Some(ep);
            }
        }
        None
    }

    /// Whether the device has endpoint `number` in `direction` with `buffering`: what a typed endpoint checks in
    /// `const { assert!(..) }`.
    pub const fn has(&self, direction: Direction, number: u8, buffering: Buffering) -> bool {
        match self.endpoint(EndpointAddress::new(number, direction)) {
            Some(ep) => matches!(
                (ep.buffering, buffering),
                (Buffering::Single, Buffering::Single) | (Buffering::Double, Buffering::Double)
            ),
            None => false,
        }
    }

    /// The interface number endpoint `address` belongs to (for requests addressed to an endpoint).
    pub const fn endpoint_interface(&self, address: EndpointAddress) -> Option<u8> {
        let mut it = Endpoints::new(self);
        while let Some((iface, ep)) = it.next() {
            if ep.address.raw() == address.raw() {
                return Some(iface);
            }
        }
        None
    }

    /// Length of the configuration descriptor with everything in it (`wTotalLength`).
    pub const fn config_len(&self) -> usize {
        self.check();
        let mut w = Writer::<0>::new();
        self.write_config(&mut w, Speed::Full);
        w.pos()
    }

    /// The configuration descriptor, followed by the IADs, interfaces, endpoints and class descriptors.
    pub const fn config_descriptor<const N: usize>(&self) -> [u8; N] {
        self.check();
        let mut w = Writer::<N>::new();
        self.write_config(&mut w, Speed::Full);
        w.finish()
    }

    /// Length of the high-speed configuration descriptor, 0 when the device is full speed only.
    pub const fn config_len_hs(&self) -> usize {
        self.check();
        if !self.high_speed {
            return 0;
        }
        let mut w = Writer::<0>::new();
        self.write_config(&mut w, Speed::High);
        w.pos()
    }

    /// The configuration descriptor at high speed: the same as [Device::config_descriptor] but for each endpoint's
    /// [Endpoint::max_packet_at] and [Endpoint::interval_at]. Empty when the device is full speed only. Served as
    /// the configuration at high speed and, with `bDescriptorType` 7, as the other-speed configuration at full
    /// speed (USB 2.0 §9.6.4: "The structure of the other_speed_configuration is identical to a configuration
    /// descriptor"); the full-speed one the other way round.
    pub const fn config_descriptor_hs<const N: usize>(&self) -> [u8; N] {
        self.check();
        let mut w = Writer::<N>::new();
        if self.high_speed {
            self.write_config(&mut w, Speed::High);
        }
        w.finish()
    }

    /// Length of the device qualifier: 10, or 0 when the device is full speed only.
    pub const fn device_qualifier_len(&self) -> usize {
        if self.high_speed { 10 } else { 0 }
    }

    /// The device qualifier (USB 2.0 §9.6.2, Table 9-9): how the device would run at the other speed. bedrock_usb
    /// devices have the same class, EP0 size and configuration count at both, so it is the device descriptor's
    /// fields that §9.6.2 names. Empty when the device is full speed only: "If a full-speed only device ...
    /// receives a GetDescriptor() request for a device_qualifier, it must respond with a request error".
    pub const fn device_qualifier<const N: usize>(&self) -> [u8; N] {
        let mut w = Writer::<N>::new();
        if self.high_speed {
            let (class, subclass, protocol) = self.class.triple();
            w.u8(10);
            w.u8(descriptor_type::DEVICE_QUALIFIER);
            w.u16(self.usb_version);
            w.u8(class);
            w.u8(subclass);
            w.u8(protocol);
            w.u8(self.max_packet_size_0);
            w.u8(1); // bNumConfigurations
            w.u8(0); // bReserved
        }
        w.finish()
    }

    const fn write_config<const N: usize>(&self, w: &mut Writer<N>, speed: Speed) {
        w.u8(9);
        w.u8(descriptor_type::CONFIGURATION);
        w.u16(0); // wTotalLength, patched below
        w.u8(self.interface_count());
        w.u8(crate::CONFIGURATION_VALUE);
        w.u8(0); // iConfiguration
        w.u8(0x80
            | if self.self_powered { 0x40 } else { 0 }
            | if self.remote_wakeup { 0x20 } else { 0 });
        w.u8((self.max_power_ma / 2) as u8);

        let iad = matches!(self.class, DeviceClass::Composite);
        let mut next_number = 0u8;
        let mut next_string = STRING_FIRST_INTERFACE;
        let mut f = 0;
        while f < self.functions.len() {
            let func = &self.functions[f];
            if iad {
                w.u8(8);
                w.u8(descriptor_type::INTERFACE_ASSOCIATION);
                w.u8(next_number); // bFirstInterface
                w.u8(function_interface_count(func));
                w.u8(func.class);
                w.u8(func.subclass);
                w.u8(func.protocol);
                w.u8(0); // iFunction
            }
            let mut i = 0;
            while i < func.interfaces.len() {
                let iface = &func.interfaces[i];
                if iface.alt_setting == 0 {
                    next_number += 1;
                }
                w.u8(9);
                w.u8(descriptor_type::INTERFACE);
                w.u8(next_number - 1);
                w.u8(iface.alt_setting);
                w.u8(iface.endpoint_count());
                w.u8(iface.class);
                w.u8(iface.subclass);
                w.u8(iface.protocol);
                if iface.string.is_some() {
                    w.u8(next_string);
                    next_string += 1;
                } else {
                    w.u8(0);
                }
                let mut k = 0;
                while k < iface.items.len() {
                    match iface.items[k] {
                        Item::Endpoint(ep) => {
                            w.u8(7);
                            w.u8(descriptor_type::ENDPOINT);
                            w.u8(ep.address.raw());
                            w.u8(ep.kind.attributes());
                            w.u16(ep.max_packet_at(speed));
                            w.u8(ep.interval_at(speed));
                        }
                        Item::Class { kind, data } => {
                            w.u8((data.len() + 2) as u8);
                            w.u8(kind);
                            w.bytes(data);
                        }
                    }
                    k += 1;
                }
                i += 1;
            }
            f += 1;
        }
        let total = w.pos();
        w.patch_u16(2, total as u16);
    }

    /// Whether the device has a BOS descriptor (`bcdUSB` 2.01 or later).
    pub const fn has_bos(&self) -> bool {
        self.usb_version >= 0x0201
    }

    /// Length of the BOS descriptor with its capabilities; 0 on a USB 2.0 device.
    pub const fn bos_len(&self) -> usize {
        let mut w = Writer::<0>::new();
        self.write_bos(&mut w);
        w.pos()
    }

    /// The BOS descriptor: the USB 2.0 extension (no LPM), then WebUSB and MS OS 2.0 platform capabilities.
    pub const fn bos_descriptor<const N: usize>(&self) -> [u8; N] {
        let mut w = Writer::<N>::new();
        self.write_bos(&mut w);
        w.finish()
    }

    const fn write_bos<const N: usize>(&self, w: &mut Writer<N>) {
        if !self.has_bos() {
            return;
        }
        self.check();
        w.u8(5);
        w.u8(descriptor_type::BOS);
        w.u16(0); // wTotalLength, patched below
        w.u8(0); // bNumDeviceCaps, patched below
        let mut caps = 1;
        // USB 2.0 extension, bmAttributes 0: no link power management (USB 2.0 LPM ECN)
        w.u8(7);
        w.u8(descriptor_type::DEVICE_CAPABILITY);
        w.u8(CAPABILITY_USB_2_0_EXTENSION);
        w.u32(0);
        if let Some(web) = self.webusb {
            caps += 1;
            w.u8(24);
            w.u8(descriptor_type::DEVICE_CAPABILITY);
            w.u8(CAPABILITY_PLATFORM);
            w.u8(0); // bReserved
            w.bytes(&WEBUSB_UUID);
            w.u16(0x0100); // bcdVersion 1.0
            w.u8(web.vendor_code);
            w.u8(web.landing_page);
        }
        if let Some(ms) = self.ms_os_20 {
            caps += 1;
            w.u8(28);
            w.u8(descriptor_type::DEVICE_CAPABILITY);
            w.u8(CAPABILITY_PLATFORM);
            w.u8(0); // bReserved
            w.bytes(&MS_OS_20_UUID);
            w.u32(ms.windows_version);
            w.u16(self.ms_os_20_len() as u16); // wMSOSDescriptorSetTotalLength
            w.u8(ms.vendor_code);
            w.u8(0); // bAltEnumCode: no alternate enumeration
        }
        let total = w.pos();
        w.patch_u16(2, total as u16);
        w.patch_u8(4, caps);
    }

    /// Length of the MS OS 2.0 descriptor set; 0 without [Device::ms_os_20].
    pub const fn ms_os_20_len(&self) -> usize {
        let mut w = Writer::<0>::new();
        self.write_ms_os_20(&mut w);
        w.pos()
    }

    /// The MS OS 2.0 descriptor set: header, device-level features, and when a function has features of its own a
    /// configuration subset (index 0) with a function subset per such function.
    pub const fn ms_os_20_set<const N: usize>(&self) -> [u8; N] {
        let mut w = Writer::<N>::new();
        self.write_ms_os_20(&mut w);
        w.finish()
    }

    const fn write_ms_os_20<const N: usize>(&self, w: &mut Writer<N>) {
        let Some(ms) = self.ms_os_20 else {
            return;
        };
        self.check();
        w.u16(10);
        w.u16(MS_OS_20_SET_HEADER);
        w.u32(ms.windows_version);
        w.u16(0); // wTotalLength, patched below
        write_features(w, ms.device);

        let mut any = false;
        let mut f = 0;
        while f < self.functions.len() {
            any |= !self.functions[f].ms_os.is_empty();
            f += 1;
        }
        if any {
            let config_at = w.pos();
            w.u16(8);
            w.u16(MS_OS_20_SUBSET_CONFIGURATION);
            // bConfigurationValue: the configuration's index, 0, not its value 1 (as Windows and embassy-usb read it)
            w.u8(0);
            w.u8(0);
            w.u16(0); // wTotalLength, patched below
            let mut first = 0u8;
            let mut f = 0;
            while f < self.functions.len() {
                let func = &self.functions[f];
                if !func.ms_os.is_empty() {
                    let at = w.pos();
                    w.u16(8);
                    w.u16(MS_OS_20_SUBSET_FUNCTION);
                    w.u8(first); // bFirstInterface
                    w.u8(0);
                    w.u16(0); // wSubsetLength, patched below
                    write_features(w, func.ms_os);
                    let len = w.pos() - at;
                    w.patch_u16(at + 6, len as u16);
                }
                first += function_interface_count(func);
                f += 1;
            }
            let len = w.pos() - config_at;
            w.patch_u16(config_at + 6, len as u16);
        }
        let total = w.pos();
        w.patch_u16(8, total as u16);
    }

    /// Panics (in `const` evaluation: fails the build) on a device the descriptors can't express.
    pub const fn check(&self) {
        assert!(
            matches!(self.max_packet_size_0, 8 | 16 | 32 | 64),
            "bMaxPacketSize0 must be 8, 16, 32 or 64"
        );
        assert!(
            !self.high_speed || self.max_packet_size_0 == 64,
            "a high-speed device has a 64-byte EP0 (USB 2.0 §5.5.3)"
        );
        assert!(self.max_power_ma <= 500, "max_power_ma is at most 500 mA");
        assert!(self.usb_version >= 0x0200, "usb_version is 0x0200 or later");
        assert!(
            self.has_bos() || (self.ms_os_20.is_none() && self.webusb.is_none()),
            "MS OS 2.0 and WebUSB need the BOS descriptor: usb_version 0x0210"
        );
        if let Str::Const(s) = self.manufacturer {
            assert!(
                string_len(s) <= 254,
                "manufacturer string over 126 UTF-16 code units"
            );
        }
        if let Str::Const(s) = self.product {
            assert!(
                string_len(s) <= 254,
                "product string over 126 UTF-16 code units"
            );
        }
        if let Str::Const(s) = self.serial {
            assert!(
                string_len(s) <= 254,
                "serial string over 126 UTF-16 code units"
            );
        }
        if let Some(ms) = self.ms_os_20 {
            assert!(
                ms.windows_version >= WINDOWS_8_1,
                "MS OS 2.0 descriptors need Windows 8.1 or later"
            );
            check_features(ms.device);
        }

        let mut strings = 0usize;
        let mut f = 0;
        while f < self.functions.len() {
            let func = &self.functions[f];
            assert!(
                !func.interfaces.is_empty(),
                "a function has at least one interface"
            );
            assert!(
                func.interfaces[0].alt_setting == 0,
                "a function's first interface entry is alternate setting 0"
            );
            assert!(
                func.ms_os.is_empty() || self.ms_os_20.is_some(),
                "a function has MS OS 2.0 features but the device has no ms_os_20"
            );
            check_features(func.ms_os);
            let mut i = 0;
            while i < func.interfaces.len() {
                let iface = &func.interfaces[i];
                if i > 0 && iface.alt_setting != 0 {
                    assert!(
                        iface.alt_setting == func.interfaces[i - 1].alt_setting + 1,
                        "alternate settings of an interface follow each other: 0, 1, 2, .."
                    );
                }
                if iface.string.is_some() {
                    strings += 1;
                }
                if let Str::Const(s) = iface.string {
                    assert!(
                        string_len(s) <= 254,
                        "interface string over 126 UTF-16 code units"
                    );
                }
                let mut k = 0;
                while k < iface.items.len() {
                    match iface.items[k] {
                        Item::Endpoint(ep) => check_endpoint(&ep),
                        Item::Class { data, .. } => {
                            assert!(
                                data.len() <= 253,
                                "a class descriptor holds at most 253 bytes"
                            )
                        }
                    }
                    k += 1;
                }
                i += 1;
            }
            f += 1;
        }
        assert!(
            strings <= (255 - STRING_FIRST_INTERFACE as usize + 1),
            "more interface strings than string indices"
        );

        // one endpoint per address, except the same endpoint in alternate settings of one interface
        let mut a = Endpoints::new(self);
        while let Some((ia, ea)) = a.next() {
            let mut b = a;
            while let Some((ib, eb)) = b.next() {
                if ea.address.raw() == eb.address.raw() {
                    assert!(
                        ia == ib && ea.same(&eb),
                        "two endpoints on one address (alternate settings of one interface may repeat the same one)"
                    );
                }
            }
        }

        let mut w = Writer::<0>::new();
        self.write_config(&mut w, Speed::Full);
        assert!(
            w.pos() <= u16::MAX as usize,
            "configuration descriptor over 65535 bytes"
        );
    }
}

/// `bDevCapabilityType` of the USB 2.0 extension (USB 3.2 Table 9-14).
pub const CAPABILITY_USB_2_0_EXTENSION: u8 = 0x02;
/// `bDevCapabilityType` of a platform capability.
pub const CAPABILITY_PLATFORM: u8 = 0x05;

/// `wDescriptorType` values of the MS OS 2.0 descriptor set (MS OS 2.0 specification, Table 9).
pub const MS_OS_20_SET_HEADER: u16 = 0;
pub const MS_OS_20_SUBSET_CONFIGURATION: u16 = 1;
pub const MS_OS_20_SUBSET_FUNCTION: u16 = 2;
pub const MS_OS_20_FEATURE_COMPATIBLE_ID: u16 = 3;
pub const MS_OS_20_FEATURE_REG_PROPERTY: u16 = 4;

const fn check_endpoint(ep: &Endpoint) {
    assert!(
        ep.address.number() != 0,
        "endpoint 0 is the control endpoint, not a class endpoint"
    );
    match ep.kind {
        EndpointKind::Bulk => {
            assert!(
                matches!(ep.max_packet, 8 | 16 | 32 | 64),
                "a full-speed bulk endpoint has 8, 16, 32 or 64 bytes"
            )
        }
        EndpointKind::Interrupt { interval } => {
            assert!(
                ep.max_packet >= 1 && ep.max_packet <= 64,
                "a full-speed interrupt endpoint has 1 to 64 bytes"
            );
            assert!(
                interval >= 1,
                "an interrupt endpoint's bInterval is 1 to 255 ms"
            );
            assert!(
                matches!(ep.buffering, Buffering::Single),
                "only bulk endpoints are double-buffered"
            );
        }
    }
}

const fn check_features(features: &[MsOsFeature]) {
    let mut i = 0;
    while i < features.len() {
        match features[i] {
            MsOsFeature::CompatibleId { id, sub_id } => {
                assert!(
                    id.len() <= 8 && sub_id.len() <= 8,
                    "a compatible ID has at most 8 characters"
                )
            }
            MsOsFeature::Registry { .. } => {}
        }
        i += 1;
    }
}

const fn write_features<const N: usize>(w: &mut Writer<N>, features: &[MsOsFeature]) {
    let mut i = 0;
    while i < features.len() {
        match features[i] {
            MsOsFeature::CompatibleId { id, sub_id } => {
                w.u16(20);
                w.u16(MS_OS_20_FEATURE_COMPATIBLE_ID);
                write_padded8(w, id);
                write_padded8(w, sub_id);
            }
            MsOsFeature::Registry { name, value } => {
                let name_len = 2 * (utf16_len(name) + 1);
                let data_len = reg_value_len(&value);
                w.u16((10 + name_len + data_len) as u16);
                w.u16(MS_OS_20_FEATURE_REG_PROPERTY);
                w.u16(reg_value_type(&value));
                w.u16(name_len as u16);
                w.utf16z(name);
                w.u16(data_len as u16);
                write_reg_value(w, &value);
            }
        }
        i += 1;
    }
}

const fn write_padded8<const N: usize>(w: &mut Writer<N>, s: &str) {
    let b = s.as_bytes();
    let mut i = 0;
    while i < 8 {
        w.u8(if i < b.len() { b[i] } else { 0 });
        i += 1;
    }
}

const fn reg_value_type(v: &RegValue) -> u16 {
    match v {
        RegValue::Sz(_) => 1,
        RegValue::ExpandSz(_) => 2,
        RegValue::Binary(_) => 3,
        RegValue::DwordLe(_) => 4,
        RegValue::DwordBe(_) => 5,
        RegValue::Link(_) => 6,
        RegValue::MultiSz(_) => 7,
    }
}

const fn reg_value_len(v: &RegValue) -> usize {
    let mut w = Writer::<0>::new();
    write_reg_value(&mut w, v);
    w.pos()
}

const fn write_reg_value<const N: usize>(w: &mut Writer<N>, v: &RegValue) {
    match *v {
        RegValue::Sz(s) | RegValue::ExpandSz(s) | RegValue::Link(s) => w.utf16z(s),
        RegValue::Binary(b) => w.bytes(b),
        RegValue::DwordLe(d) => w.u32(d),
        RegValue::DwordBe(d) => w.u32(d.swap_bytes()),
        RegValue::MultiSz(list) => {
            let mut i = 0;
            while i < list.len() {
                w.utf16z(list[i]);
                i += 1;
            }
            w.u16(0);
        }
    }
}

const fn function_interface_count(f: &Function) -> u8 {
    let mut n = 0;
    let mut i = 0;
    while i < f.interfaces.len() {
        if f.interfaces[i].alt_setting == 0 {
            n += 1;
        }
        i += 1;
    }
    n
}

/// An interface entry with the number and string index the descriptors give it.
#[derive(Copy, Clone)]
pub(crate) struct InterfaceAt {
    pub(crate) number: u8,
    pub(crate) string_index: u8,
    pub(crate) interface: &'static Interface,
}

/// Walks the interface entries of a device in descriptor order (a `const` iterator).
#[derive(Copy, Clone)]
pub(crate) struct Interfaces {
    functions: &'static [Function],
    f: usize,
    i: usize,
    number: u8,
    string: u8,
}

impl Interfaces {
    pub(crate) const fn new(device: &Device) -> Self {
        Interfaces {
            functions: device.functions,
            f: 0,
            i: 0,
            number: 0,
            string: STRING_FIRST_INTERFACE,
        }
    }

    pub(crate) const fn next(&mut self) -> Option<InterfaceAt> {
        while self.f < self.functions.len() {
            let func = &self.functions[self.f];
            if self.i < func.interfaces.len() {
                let iface = &func.interfaces[self.i];
                self.i += 1;
                if iface.alt_setting == 0 {
                    self.number += 1;
                }
                let string_index = if iface.string.is_some() {
                    self.string += 1;
                    self.string - 1
                } else {
                    0
                };
                return Some(InterfaceAt {
                    number: self.number - 1,
                    string_index,
                    interface: iface,
                });
            }
            self.f += 1;
            self.i = 0;
        }
        None
    }
}

/// Walks every endpoint of a device with its interface number, alternate settings included.
#[derive(Copy, Clone)]
pub(crate) struct Endpoints {
    interfaces: Interfaces,
    current: Option<InterfaceAt>,
    k: usize,
}

impl Endpoints {
    pub(crate) const fn new(device: &Device) -> Self {
        Endpoints {
            interfaces: Interfaces::new(device),
            current: None,
            k: 0,
        }
    }

    pub(crate) const fn next(&mut self) -> Option<(u8, Endpoint)> {
        loop {
            if let Some(at) = self.current {
                let items = at.interface.items;
                while self.k < items.len() {
                    let item = items[self.k];
                    self.k += 1;
                    if let Item::Endpoint(ep) = item {
                        return Some((at.number, ep));
                    }
                }
            }
            self.current = self.interfaces.next();
            self.k = 0;
            if self.current.is_none() {
                return None;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Buffering::{Double, Single};
    use std::vec;

    const fn bulk_pair(n: u8) -> [Item; 2] {
        [
            Item::Endpoint(Endpoint::bulk_out(n, 64, Single)),
            Item::Endpoint(Endpoint::bulk_in(n, 64, Single)),
        ]
    }
    const PAIR1: [Item; 2] = bulk_pair(1);

    const SIMPLE: Device = Device {
        class: DeviceClass::PerInterface,
        usb_version: 0x0200,
        bcd_device: 0x0123,
        manufacturer: Str::Const("ab"),
        serial: Str::Runtime,
        functions: &[Function {
            interfaces: &[Interface {
                string: Str::Const("x"),
                items: &PAIR1,
                ..Interface::vendor()
            }],
            ..Function::vendor()
        }],
        ..Device::new(0x1209, 0x0001)
    };

    #[test]
    fn device_descriptor() {
        assert_eq!(
            SIMPLE.device_descriptor(),
            [
                18, 1, 0x00, 0x02, 0, 0, 0, 64, 0x09, 0x12, 0x01, 0x00, 0x23, 0x01, 1, 0, 3, 1
            ]
        );
        let d = Device {
            class: DeviceClass::Custom {
                class: 2,
                subclass: 3,
                protocol: 4,
            },
            ..SIMPLE
        };
        assert_eq!(&d.device_descriptor()[4..7], &[2, 3, 4]);
    }

    #[test]
    fn config_without_iad() {
        const C: [u8; SIMPLE.config_len()] = SIMPLE.config_descriptor();
        #[rustfmt::skip]
        let want = [
            9, 2, 32, 0, 1, 1, 0, 0x80, 50,          // configuration: 32 bytes, 1 interface, bus powered, 100 mA
            9, 4, 0, 0, 2, 0xff, 0, 0, 4,             // interface 0, alt 0, 2 endpoints, vendor, string 4
            7, 5, 0x01, 0x02, 64, 0, 0,               // EP1 OUT bulk 64
            7, 5, 0x81, 0x02, 64, 0, 0,               // EP1 IN bulk 64
        ];
        assert_eq!(C, want);
    }

    #[test]
    fn config_attributes() {
        let d = Device {
            self_powered: true,
            remote_wakeup: true,
            max_power_ma: 500,
            ..SIMPLE
        };
        let c: [u8; 32] = d.config_descriptor();
        assert_eq!((c[7], c[8]), (0xE0, 250));
    }

    const CDC_FUNC: &[Function] = &[
        Function {
            class: 2,
            subclass: 2,
            protocol: 0,
            interfaces: &[
                Interface {
                    class: 2,
                    subclass: 2,
                    items: &[
                        Item::Class {
                            kind: 0x24,
                            data: &[0x00, 0x10, 0x01],
                        }, // CDC header 1.10
                        Item::Endpoint(Endpoint::interrupt_in(2, 8, 255)),
                    ],
                    ..Interface::vendor()
                },
                Interface {
                    class: 0x0a,
                    ..Interface::vendor()
                },
                Interface {
                    alt_setting: 1,
                    class: 0x0a,
                    string: Str::Runtime,
                    items: &PAIR1,
                    ..Interface::vendor()
                },
            ],
            ms_os: &[],
        },
        Function {
            interfaces: &[Interface {
                string: Str::Const("b"),
                items: &[Item::Endpoint(Endpoint::bulk_in(3, 32, Double))],
                ..Interface::vendor()
            }],
            ..Function::vendor()
        },
    ];

    #[test]
    fn config_with_iads_alts_and_class_blobs() {
        const D: Device = Device {
            functions: CDC_FUNC,
            ..Device::new(1, 2)
        };
        const C: [u8; D.config_len()] = D.config_descriptor();
        #[rustfmt::skip]
        let want: &[u8] = &[
            9, 2, 94, 0, 3, 1, 0, 0x80, 50,
            8, 11, 0, 2, 2, 2, 0, 0,                  // IAD: interfaces 0-1, CDC
            9, 4, 0, 0, 1, 2, 2, 0, 0,                // interface 0, 1 endpoint
            5, 0x24, 0x00, 0x10, 0x01,                // class blob before the endpoint
            7, 5, 0x82, 0x03, 8, 0, 255,              // EP2 IN interrupt 8, 255 ms
            9, 4, 1, 0, 0, 0x0a, 0, 0, 0,             // interface 1 alt 0, no endpoints
            9, 4, 1, 1, 2, 0x0a, 0, 0, 4,             // interface 1 alt 1, runtime string 4
            7, 5, 0x01, 0x02, 64, 0, 0,
            7, 5, 0x81, 0x02, 64, 0, 0,
            8, 11, 2, 1, 0xff, 0, 0, 0,               // IAD: interface 2
            9, 4, 2, 0, 1, 0xff, 0, 0, 5,             // interface 2, string 5
            7, 5, 0x83, 0x02, 32, 0, 0,
        ];
        assert_eq!(&C[..], want);
        assert_eq!(D.interface_count(), 3);
        assert_eq!(
            (
                D.alt_settings(0),
                D.alt_settings(1),
                D.alt_settings(2),
                D.alt_settings(3)
            ),
            (1, 2, 1, 0)
        );
        assert_eq!(D.string(4), Some(Str::Runtime));
        assert_eq!(D.string(5), Some(Str::Const("b")));
        assert_eq!(D.string(6), None);
        assert_eq!(D.string(1), None);
        assert_eq!(
            D.endpoint_interface(EndpointAddress::new(1, Direction::In)),
            Some(1)
        );
        assert_eq!(
            D.endpoint_interface(EndpointAddress::new(3, Direction::In)),
            Some(2)
        );
        assert!(D.has(Direction::In, 3, Double));
        assert!(!D.has(Direction::In, 3, Single));
        assert!(!D.has(Direction::Out, 3, Double));
        assert!(D.has(Direction::Out, 1, Single));
        D.check();
    }

    #[test]
    fn strings() {
        assert_eq!(STRING0, [4, 3, 0x09, 0x04]);
        const S: [u8; string_len("Hé€😀")] = string_descriptor("Hé€😀");
        // H, é U+00E9, € U+20AC, 😀 U+1F600 = D83D DE00
        assert_eq!(
            S,
            [12, 3, b'H', 0, 0xe9, 0, 0xac, 0x20, 0x3d, 0xd8, 0x00, 0xde]
        );
        assert_eq!(string_len(""), 2);
    }

    #[test]
    fn bos_usb20_is_empty() {
        assert_eq!(SIMPLE.bos_len(), 0);
        assert_eq!(SIMPLE.ms_os_20_len(), 0);
    }

    #[test]
    fn bos_and_ms_os_20() {
        const GUIDS: &[&str] = &["{A}"];
        const D: Device = Device {
            ms_os_20: Some(MsOs20 {
                vendor_code: 0x21,
                windows_version: WINDOWS_8_1,
                device: &[MsOsFeature::CompatibleId {
                    id: "WINUSB",
                    sub_id: "",
                }],
            }),
            webusb: Some(WebUsb {
                vendor_code: 0x22,
                landing_page: 1,
            }),
            functions: &[
                Function {
                    interfaces: &[Interface::vendor()],
                    ..Function::vendor()
                },
                Function {
                    interfaces: &[Interface::vendor()],
                    ms_os: &[MsOsFeature::Registry {
                        name: "G",
                        value: RegValue::MultiSz(GUIDS),
                    }],
                    ..Function::vendor()
                },
            ],
            ..Device::new(1, 2)
        };
        const SET: [u8; D.ms_os_20_len()] = D.ms_os_20_set();
        #[rustfmt::skip]
        let want_set: &[u8] = &[
            10, 0, 0, 0, 0x00, 0x00, 0x03, 0x06, 70, 0,     // set header, Windows 8.1, 70 bytes
            20, 0, 3, 0, b'W', b'I', b'N', b'U', b'S', b'B', 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            8, 0, 1, 0, 0, 0, 40, 0,                         // configuration subset, 40 bytes
            8, 0, 2, 0, 1, 0, 32, 0,                         // function subset from interface 1, 32 bytes
            24, 0, 4, 0, 7, 0, 4, 0, b'G', 0, 0, 0,          // registry: REG_MULTI_SZ, name "G"
            10, 0, b'{', 0, b'A', 0, b'}', 0, 0, 0, 0, 0,
        ];
        assert_eq!(&SET[..], want_set);

        const BOS: [u8; D.bos_len()] = D.bos_descriptor();
        let mut want_bos = vec![5, 15, 64, 0, 3, 7, 16, 2, 0, 0, 0, 0];
        want_bos.extend([24, 16, 5, 0]);
        want_bos.extend(WEBUSB_UUID);
        want_bos.extend([0x00, 0x01, 0x22, 1]);
        want_bos.extend([28, 16, 5, 0]);
        want_bos.extend(MS_OS_20_UUID);
        want_bos.extend([0x00, 0x00, 0x03, 0x06, 70, 0, 0x21, 0]);
        assert_eq!(&BOS[..], &want_bos[..]);
    }

    #[test]
    fn reg_values() {
        let mut w = Writer::<16>::new();
        write_reg_value(&mut w, &RegValue::DwordBe(0x01020304));
        write_reg_value(&mut w, &RegValue::DwordLe(0x01020304));
        write_reg_value(&mut w, &RegValue::Sz("a"));
        write_reg_value(&mut w, &RegValue::Binary(&[9, 8, 7, 6]));
        assert_eq!(
            w.finish(),
            [1, 2, 3, 4, 4, 3, 2, 1, b'a', 0, 0, 0, 9, 8, 7, 6]
        );
        assert_eq!(reg_value_type(&RegValue::Link("")), 6);
    }

    #[test]
    #[should_panic(expected = "two endpoints on one address")]
    fn duplicate_endpoint() {
        const F: &[Function] = &[
            Function {
                interfaces: &[Interface {
                    items: &PAIR1,
                    ..Interface::vendor()
                }],
                ..Function::vendor()
            },
            Function {
                interfaces: &[Interface {
                    items: &PAIR1,
                    ..Interface::vendor()
                }],
                ..Function::vendor()
            },
        ];
        Device {
            functions: F,
            ..Device::new(1, 2)
        }
        .check();
    }

    #[test]
    #[should_panic(expected = "bulk endpoint has 8, 16, 32 or 64")]
    fn bad_bulk_size() {
        const F: &[Function] = &[Function {
            interfaces: &[Interface {
                items: &[Item::Endpoint(Endpoint::bulk_in(1, 100, Single))],
                ..Interface::vendor()
            }],
            ..Function::vendor()
        }];
        Device {
            functions: F,
            ..Device::new(1, 2)
        }
        .check();
    }

    #[test]
    #[should_panic(expected = "need the BOS descriptor")]
    fn ms_os_needs_bos() {
        Device {
            usb_version: 0x0200,
            ms_os_20: Some(MsOs20 {
                vendor_code: 1,
                windows_version: WINDOWS_8_1,
                device: &[],
            }),
            ..Device::new(1, 2)
        }
        .check();
    }

    #[test]
    #[should_panic(expected = "no ms_os_20")]
    fn function_ms_os_needs_set() {
        const F: &[Function] = &[Function {
            interfaces: &[Interface::vendor()],
            ms_os: &[MsOsFeature::CompatibleId {
                id: "WINUSB",
                sub_id: "",
            }],
            ..Function::vendor()
        }];
        Device {
            functions: F,
            ..Device::new(1, 2)
        }
        .check();
    }

    #[test]
    #[should_panic(expected = "only bulk endpoints are double-buffered")]
    fn double_interrupt() {
        const F: &[Function] = &[Function {
            interfaces: &[Interface {
                items: &[Item::Endpoint(Endpoint {
                    buffering: Double,
                    ..Endpoint::interrupt_in(1, 8, 1)
                })],
                ..Interface::vendor()
            }],
            ..Function::vendor()
        }];
        Device {
            functions: F,
            ..Device::new(1, 2)
        }
        .check();
    }
}
