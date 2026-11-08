//! The WireWeaver class (feature `ww`): a WireWeaver device API served over two bulk endpoints.
//!
//! - [ww_function!] writes the function's descriptors the way `wire_weaver_usb_embassy` does: vendor class 0xFF /
//!   0x00, the framing as the interface protocol, the API id as the interface string, a bulk OUT and a bulk IN
//!   endpoint, the vendor descriptors 0x40-0x42, and the MS OS 2.0 features that make Windows bind WinUSB. The
//!   constants come from `ww_link` (TR-23), the one definition both stacks and the host use.
//! - [Sender] and [Receiver] are `ww_device::PacketSink` / `PacketSource` on the typed bulk endpoints of the
//!   driver, to be used with `ww_device::FramedTx` / `FramedRx`. The sender gives up after a write timeout, so a
//!   host that is gone without disconnecting is noticed.
//!
//! ```ignore
//! const DEVICE: Device = Device {
//!     ms_os_20: Some(bedrock_usb::ww::MS_OS_20),
//!     functions: &[bedrock_usb::ww_function! {
//!         out: 1, in: 2, max_packet: 64, buffering: Double,
//!         user_crate: "my_api", user_version: (0, 1, 0), api_id: Str::Runtime,
//!     }],
//!     ..Device::new(0x1209, 0x0001)
//! };
//! bedrock_usb::descriptors!(Desc = DEVICE);
//! // later: let (tx, rx) = (Sender::new(usb.bulk_in::<2, Double>(), timeout), Receiver::new(usb.bulk_out::<1, Double>()));
//! ```

use core::future::Future;
use core::marker::PhantomData;

use embassy_futures::select::{Either, select};
use ww_device::{PacketSink, PacketSource};
use ww_link::{UsbFraming, UsbFramingProtocol};

use crate::desc::{MsOs20, MsOsFeature, RegValue, WINDOWS_8_1};
use crate::v4::{BulkIn, BulkOut, EndpointError, Regs};

pub use ww_link::{
    CUSTOM_DESCRIPTOR_TYPE_VENDOR_SPECIFIC_SELF_ID_VERSION,
    CUSTOM_DESCRIPTOR_TYPE_VENDOR_SPECIFIC_USER_CRATE,
    CUSTOM_DESCRIPTOR_TYPE_VENDOR_SPECIFIC_USER_VERSION, USB_DEVICE_CLASS_GUID,
};

#[doc(hidden)]
pub use ww_link as __ww_link;

/// Names this stack in descriptor 0x40, where `wire_weaver_usb_embassy` has `wwue`.
pub const SELF_ID_TAG: [u8; 4] = *b"bdus";

/// Descriptor 0x40's payload: [SELF_ID_TAG] and this crate's version.
pub const SELF_ID: [u8; 7] = ww_link::usb_self_id_version_descriptor(
    SELF_ID_TAG,
    parse_u32(env!("CARGO_PKG_VERSION_MAJOR")),
    parse_u32(env!("CARGO_PKG_VERSION_MINOR")),
    parse_u32(env!("CARGO_PKG_VERSION_PATCH")),
);

const fn parse_u32(s: &str) -> u32 {
    let b = s.as_bytes();
    let mut v = 0u32;
    let mut i = 0;
    while i < b.len() {
        v = v * 10 + (b[i] - b'0') as u32;
        i += 1;
    }
    v
}

/// The MS OS 2.0 features of a WireWeaver function, at device level and in the function subset: WinUSB and the
/// WireWeaver device interface GUID.
pub const MS_OS_FEATURES: &[MsOsFeature] = &[
    MsOsFeature::CompatibleId {
        id: "WINUSB",
        sub_id: "",
    },
    MsOsFeature::Registry {
        name: "DeviceInterfaceGUIDs",
        value: RegValue::MultiSz(&[USB_DEVICE_CLASS_GUID]),
    },
];

/// The device's MS OS 2.0 descriptor set for a device whose only Windows-specific need is WinUSB on its WireWeaver
/// function (vendor code 0, as `wire_weaver_usb_embassy` uses).
pub const MS_OS_20: MsOs20 = MsOs20 {
    vendor_code: 0,
    windows_version: WINDOWS_8_1,
    device: MS_OS_FEATURES,
};

/// Descriptor 1's version payload: major.minor.patch saturated to a byte (`ww_link::usb_version_descriptor`).
pub const fn user_version(major: u32, minor: u32, patch: u32) -> [u8; 3] {
    ww_link::usb_version_descriptor(major, minor, patch)
}

/// `bInterfaceProtocol` of a WireWeaver interface with framing `F`.
pub const fn protocol<F: UsbFramingProtocol>() -> u8 {
    F::INTERFACE_PROTOCOL
}

/// A WireWeaver [Function](crate::desc::Function), as a `const` expression.
///
/// Fields, in this order (the last three optional): `out` / `in` endpoint numbers, `max_packet`, `buffering`
/// ([Buffering](crate::Buffering) variant name or value), `user_crate` (`FullVersion::crate_id`), `user_version`
/// `(major, minor, patch)`, `api_id` (a [Str](crate::desc::Str): the API id constant, or `Str::Runtime` when the
/// firmware adds a label), then `framing: Type` (default `ww_link::UsbFraming`; `UsbSeqFraming` for the
/// `seq-prefix` variant) and `self_id: [u8; 7]` (default [SELF_ID]).
#[macro_export]
macro_rules! ww_function {
    (
        out: $out:expr, in: $in:expr, max_packet: $mp:expr, buffering: $buf:expr,
        user_crate: $crate_id:expr, user_version: ($ma:expr, $mi:expr, $pa:expr), api_id: $api:expr $(,)?
    ) => {
        $crate::ww_function! {
            out: $out, in: $in, max_packet: $mp, buffering: $buf,
            user_crate: $crate_id, user_version: ($ma, $mi, $pa), api_id: $api,
            framing: $crate::ww::__ww_link::UsbFraming, self_id: $crate::ww::SELF_ID,
        }
    };
    (
        out: $out:expr, in: $in:expr, max_packet: $mp:expr, buffering: $buf:expr,
        user_crate: $crate_id:expr, user_version: ($ma:expr, $mi:expr, $pa:expr), api_id: $api:expr,
        framing: $framing:ty $(,)?
    ) => {
        $crate::ww_function! {
            out: $out, in: $in, max_packet: $mp, buffering: $buf,
            user_crate: $crate_id, user_version: ($ma, $mi, $pa), api_id: $api,
            framing: $framing, self_id: $crate::ww::SELF_ID,
        }
    };
    (
        out: $out:expr, in: $in:expr, max_packet: $mp:expr, buffering: $buf:expr,
        user_crate: $crate_id:expr, user_version: ($ma:expr, $mi:expr, $pa:expr), api_id: $api:expr,
        framing: $framing:ty, self_id: $self_id:expr $(,)?
    ) => {
        $crate::desc::Function {
            class: 0xFF,
            subclass: 0,
            protocol: $crate::ww::protocol::<$framing>(),
            interfaces: &[$crate::desc::Interface {
                protocol: $crate::ww::protocol::<$framing>(),
                string: $api,
                items: &[
                    $crate::desc::Item::Endpoint($crate::desc::Endpoint::bulk_out($out, $mp, $buf)),
                    $crate::desc::Item::Endpoint($crate::desc::Endpoint::bulk_in($in, $mp, $buf)),
                    $crate::desc::Item::Class {
                        kind: $crate::ww::CUSTOM_DESCRIPTOR_TYPE_VENDOR_SPECIFIC_SELF_ID_VERSION,
                        data: &$self_id,
                    },
                    $crate::desc::Item::Class {
                        kind: $crate::ww::CUSTOM_DESCRIPTOR_TYPE_VENDOR_SPECIFIC_USER_CRATE,
                        data: $crate_id.as_bytes(),
                    },
                    $crate::desc::Item::Class {
                        kind: $crate::ww::CUSTOM_DESCRIPTOR_TYPE_VENDOR_SPECIFIC_USER_VERSION,
                        data: &$crate::ww::user_version($ma, $mi, $pa),
                    },
                ],
                ..$crate::desc::Interface::vendor()
            }],
            ms_os: $crate::ww::MS_OS_FEATURES,
        }
    };
}

/// A bulk IN endpoint the [Sender] can write packets to.
pub trait PacketIn {
    type Error;
    /// The error a write that timed out gives.
    fn timed_out() -> Self::Error;
    fn max_packet(&self) -> u16;
    fn write(&mut self, data: &[u8]) -> impl Future<Output = Result<(), Self::Error>>;
    fn wait_enabled(&mut self) -> impl Future<Output = ()>;
}

/// A bulk OUT endpoint the [Receiver] can read packets from.
pub trait PacketOut {
    type Error;
    fn max_packet(&self) -> u16;
    fn read(&mut self, buf: &mut [u8]) -> impl Future<Output = Result<usize, Self::Error>>;
    fn wait_enabled(&mut self) -> impl Future<Output = ()>;
}

macro_rules! packet_in {
    ($buffering:ty) => {
        impl<R: Regs + Copy, const EP: u8> PacketIn for BulkIn<R, EP, $buffering> {
            type Error = EndpointError;
            fn timed_out() -> EndpointError {
                // as wire_weaver_usb_embassy: a host that stopped reading looks like a disabled endpoint
                EndpointError::Disabled
            }
            fn max_packet(&self) -> u16 {
                BulkIn::max_packet(self)
            }
            fn write(&mut self, data: &[u8]) -> impl Future<Output = Result<(), EndpointError>> {
                BulkIn::<R, EP, $buffering>::write(self, data)
            }
            fn wait_enabled(&mut self) -> impl Future<Output = ()> {
                BulkIn::wait_enabled(self)
            }
        }
    };
}
macro_rules! packet_out {
    ($buffering:ty) => {
        impl<R: Regs + Copy, const EP: u8> PacketOut for BulkOut<R, EP, $buffering> {
            type Error = EndpointError;
            fn max_packet(&self) -> u16 {
                BulkOut::max_packet(self)
            }
            fn read(
                &mut self,
                buf: &mut [u8],
            ) -> impl Future<Output = Result<usize, EndpointError>> {
                BulkOut::<R, EP, $buffering>::read(self, buf)
            }
            fn wait_enabled(&mut self) -> impl Future<Output = ()> {
                BulkOut::wait_enabled(self)
            }
        }
    };
}
packet_in!(crate::buffer::Single);
packet_in!(crate::buffer::Double);
packet_out!(crate::buffer::Single);
packet_out!(crate::buffer::Double);

/// A timer for the write timeout, so bedrock_usb needs no particular executor or time driver.
pub trait Sleep {
    /// Completes after the timeout. Called anew for every write.
    fn sleep(&self) -> impl Future<Output = ()>;
}

/// The write timeout on embassy-time (feature `embassy-time`).
#[cfg(feature = "embassy-time")]
impl Sleep for embassy_time::Duration {
    fn sleep(&self) -> impl Future<Output = ()> {
        embassy_time::Timer::after(*self)
    }
}

/// `ww_device::PacketSink` on a bulk IN endpoint: one packet per write, failing with [PacketIn::timed_out] when the
/// host does not take it within the timeout.
pub struct Sender<E: PacketIn, T: Sleep, F: UsbFramingProtocol = UsbFraming> {
    ep: E,
    timeout: T,
    _framing: PhantomData<F>,
}

impl<E: PacketIn, T: Sleep, F: UsbFramingProtocol> Sender<E, T, F> {
    pub fn new(ep: E, timeout: T) -> Self {
        Sender {
            ep,
            timeout,
            _framing: PhantomData,
        }
    }

    pub fn max_packet_size(&self) -> u16 {
        self.ep.max_packet()
    }

    /// Waits for the host to enable this interface (SET_CONFIGURATION / SET_INTERFACE).
    pub async fn wait_connection(&mut self) {
        self.ep.wait_enabled().await
    }
}

impl<E: PacketIn, T: Sleep, F: UsbFramingProtocol> PacketSink for Sender<E, T, F> {
    type Error = E::Error;
    type Framing = F;

    async fn write_packet(&mut self, packet: &[u8]) -> Result<(), Self::Error> {
        match select(self.ep.write(packet), self.timeout.sleep()).await {
            Either::First(r) => r,
            Either::Second(()) => Err(E::timed_out()),
        }
    }
}

/// `ww_device::PacketSource` on a bulk OUT endpoint.
pub struct Receiver<E: PacketOut, F: UsbFramingProtocol = UsbFraming> {
    ep: E,
    _framing: PhantomData<F>,
}

impl<E: PacketOut, F: UsbFramingProtocol> Receiver<E, F> {
    pub fn new(ep: E) -> Self {
        Receiver {
            ep,
            _framing: PhantomData,
        }
    }

    pub fn max_packet_size(&self) -> u16 {
        self.ep.max_packet()
    }
}

impl<E: PacketOut, F: UsbFramingProtocol> PacketSource for Receiver<E, F> {
    type Error = E::Error;
    type Framing = F;

    fn max_packet_len(&self) -> usize {
        self.ep.max_packet() as usize
    }

    async fn read_packet(&mut self, buf: &mut [u8]) -> Result<usize, Self::Error> {
        self.ep.read(buf).await
    }

    async fn wait_connected(&mut self) {
        self.ep.wait_enabled().await
    }
}

#[cfg(test)]
mod tests;
