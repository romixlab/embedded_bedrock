//! The control endpoint (EP0) as a pure state machine: events in, what to do with EP0 out.
//!
//! [Control] knows no registers. The driver tells it what happened on EP0 and does what it answers:
//!
//! - [Control::setup] when a SETUP packet arrived, [Control::in_complete] when an IN packet on EP0 was
//!   acknowledged, [Control::out_data] when an OUT packet on EP0 arrived (data or the zero-length status),
//!   [Control::reset] on a bus reset;
//! - each returns an [Action]: what to arm on EP0 next ([Ep0]) and what to change on the other endpoints or the
//!   address register first ([Effect]). For [Ep0::Send] the packet's bytes come from [Control::packet], written
//!   straight into the packet buffer, so no descriptor is copied into RAM.
//!
//! The standard requests of USB 2.0 chapter 9 are answered from the [Descriptors] tables: descriptors (device,
//! configuration, strings, BOS), addresses, the configuration, alternate settings, status, remote wakeup and
//! endpoint halt. So are the MS OS 2.0 descriptor set (its vendor request, `wIndex` 7) and WebUSB's GET_URL (the
//! URL from [Handler::webusb_url]). Everything else goes to the class [Handler]s, a tuple dispatched statically:
//! the first that doesn't answer [InResponse::Unhandled] / [OutResponse::Unhandled] has it, and a request nobody
//! takes is stalled.
//!
//! The rules it keeps, from USB 2.0:
//! - §5.5.3: an IN data stage ends after exactly `wLength` bytes or with a short packet, so data shorter than
//!   `wLength` whose length is a multiple of the EP0 packet size ends with a zero-length packet, data equal to
//!   `wLength` doesn't.
//! - §9.4.6: the device takes its new address after the status stage of SET_ADDRESS ([Effect::SetAddress] comes
//!   from the [Control::in_complete] of that status stage).
//! - §9.4.5 and §9.1.1.5: SET_CONFIGURATION and SET_INTERFACE reset the halt feature and the data toggle of the
//!   endpoints they touch, CLEAR_FEATURE(ENDPOINT_HALT) always resets the data toggle ([Effect::Halt] with
//!   `halted: false` even on an endpoint that wasn't halted).
//! - §9.6.2: a full-speed-only device has no device qualifier, GET_DESCRIPTOR(DEVICE_QUALIFIER) is a request error.
//!
//! How the driver arms EP0 for each [Ep0] (RM0444 §37.5.2 for usb_v4) is the driver's part.

use core::marker::PhantomData;

use crate::desc::{self, Endpoints, Interfaces, Str};
use crate::{
    CONFIGURATION_VALUE, Descriptors, Direction, EndpointAddress, Recipient, RequestKind, Setup,
    descriptor_type, request,
};

/// Interfaces a device may have for [Control]'s alternate setting table (interface numbers, alternate settings
/// not counted).
pub const MAX_INTERFACES: usize = 8;

/// `bDescriptorType` of WebUSB's URL descriptor (WebUSB specification §4.3.1).
pub const WEBUSB_URL: u8 = 3;

/// The device states of USB 2.0 §9.1.1 that the control machine tells apart.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum DeviceState {
    /// After a bus reset: address 0.
    Default,
    /// Has an address, not configured.
    Address,
    /// Configuration [CONFIGURATION_VALUE] is set: the class endpoints are enabled.
    Configured,
}

/// What to arm on EP0 next.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum Ep0 {
    /// Nothing until the next SETUP: the transfer is over (both directions NAK; a SETUP is taken anyway).
    Idle,
    /// Request error: STALL both directions until the next SETUP.
    Stall,
    /// Send the next IN data packet, [Control::packet]'s bytes (possibly none: a zero-length packet). `last`: the
    /// data stage ends with it, so OUT is NAK while it is armed, else STALL (RM0444 §37.5.2).
    Send { last: bool },
    /// The IN data stage is over: take the zero-length OUT status packet (usb_v4: STATUS_OUT set).
    ReceiveStatus,
    /// Take the next OUT data packet; `last` as for [Ep0::Send], for the IN direction.
    Receive { last: bool },
    /// Send the zero-length IN status packet of a request without data stage or after an OUT data stage.
    SendStatus,
}

/// What to change outside EP0, before arming EP0 as the [Action] says.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum Effect {
    None,
    /// Write the device address (the status stage of SET_ADDRESS is over).
    SetAddress(u8),
    /// SET_CONFIGURATION: `true` enables every endpoint of alternate setting 0 of each interface, un-halted with
    /// DATA0 (also when the device was configured already), `false` disables all class endpoints.
    Configure(bool),
    /// SET_INTERFACE: the endpoints of `interface` go to alternate setting `alt` ([Control::is_active] says which
    /// are on), un-halted with DATA0.
    SetInterface {
        interface: u8,
        alt: u8,
    },
    /// SET_FEATURE / CLEAR_FEATURE(ENDPOINT_HALT): stall the endpoint, or un-stall it and reset its data toggle to
    /// DATA0 (for a double-buffered endpoint the buffer selection with it).
    Halt {
        endpoint: EndpointAddress,
        halted: bool,
    },
}

/// The answer to an event: first the [Effect], then arm EP0.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub struct Action {
    pub ep0: Ep0,
    pub effect: Effect,
}

impl Action {
    const fn ep0(ep0: Ep0) -> Self {
        Action {
            ep0,
            effect: Effect::None,
        }
    }
}

/// A class handler's answer to a request with an IN data stage (or none, `wLength` 0).
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum InResponse {
    /// Send these bytes (cut to `wLength`).
    Static(&'static [u8]),
    /// Send the first `n` bytes of the buffer [Handler::control_in] got (cut to `wLength`).
    Data(usize),
    /// The request is this handler's but not valid now: request error (STALL).
    Reject,
    /// Not this handler's request: the next one is asked.
    Unhandled,
}

/// A class handler's answer to a request with an OUT data stage or without data stage.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum OutResponse {
    /// Done: the status stage completes.
    Accept,
    /// Request error (STALL).
    Reject,
    /// Not this handler's request.
    Unhandled,
}

/// `bScheme` of a WebUSB URL descriptor.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum UrlScheme {
    /// `http://` in front of the URL.
    Http = 0,
    /// `https://` in front of the URL.
    Https = 1,
    /// The URL is complete as it is.
    Raw = 255,
}

/// A WebUSB URL: what GET_URL serves.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct Url<'a> {
    pub scheme: UrlScheme,
    /// UTF-8, without the scheme; at most 252 bytes are sent.
    pub url: &'a str,
}

/// A class (or the firmware itself) on the control endpoint. Every method has a default that does nothing or
/// leaves the request to the next handler, so a class implements only what it uses.
///
/// Handlers combine as a tuple, `(WwControl, DapControl)`: each event goes to every member in order, a request to
/// the first that takes it, with no `dyn`. `()` is the device without class requests.
///
/// The string and URL methods are asked again for each packet of the descriptor, so they must give the same text
/// for the whole transfer (a serial number formatted once at boot, a label that changes only between transfers).
pub trait Handler {
    /// A bus reset: the device is back in the default state.
    fn reset(&mut self) {}
    /// SET_CONFIGURATION: configured (`true`) or back to the address state.
    fn configured(&mut self, _configured: bool) {}
    /// The bus was suspended (`true`) or resumed; the control machine doesn't call it, the driver does.
    fn suspended(&mut self, _suspended: bool) {}
    /// SET_INTERFACE on one of the device's interfaces, already accepted.
    fn set_alternate_setting(&mut self, _interface: u8, _alt: u8) {}
    /// A class, vendor or interface request with an IN data stage, or one without data stage that is IN. `buf`
    /// holds the bytes for [InResponse::Data].
    fn control_in(&mut self, _setup: &Setup, _buf: &mut [u8]) -> InResponse {
        InResponse::Unhandled
    }
    /// A class, vendor or interface request with an OUT data stage (`data`, after the whole stage arrived) or
    /// without data stage (`data` empty).
    fn control_out(&mut self, _setup: &Setup, _data: &[u8]) -> OutResponse {
        OutResponse::Unhandled
    }
    /// The text of string descriptor `index` that the device declares [Str::Runtime]; `None` stalls the request.
    /// At most 126 UTF-16 code units are sent.
    fn string(&self, _index: u8) -> Option<&str> {
        None
    }
    /// WebUSB's GET_URL for URL `index` (`iLandingPage` is one), on a device with [desc::WebUsb].
    fn webusb_url(&self, _index: u8) -> Option<Url<'_>> {
        None
    }
}

impl Handler for () {}

macro_rules! tuple_handler {
    ($($h:ident $i:tt),+) => {
        impl<$($h: Handler),+> Handler for ($($h,)+) {
            fn reset(&mut self) {
                $(self.$i.reset();)+
            }
            fn configured(&mut self, configured: bool) {
                $(self.$i.configured(configured);)+
            }
            fn suspended(&mut self, suspended: bool) {
                $(self.$i.suspended(suspended);)+
            }
            fn set_alternate_setting(&mut self, interface: u8, alt: u8) {
                $(self.$i.set_alternate_setting(interface, alt);)+
            }
            fn control_in(&mut self, setup: &Setup, buf: &mut [u8]) -> InResponse {
                $(match self.$i.control_in(setup, buf) {
                    InResponse::Unhandled => {}
                    r => return r,
                })+
                InResponse::Unhandled
            }
            fn control_out(&mut self, setup: &Setup, data: &[u8]) -> OutResponse {
                $(match self.$i.control_out(setup, data) {
                    OutResponse::Unhandled => {}
                    r => return r,
                })+
                OutResponse::Unhandled
            }
            fn string(&self, index: u8) -> Option<&str> {
                $(if let Some(s) = self.$i.string(index) {
                    return Some(s);
                })+
                None
            }
            fn webusb_url(&self, index: u8) -> Option<Url<'_>> {
                $(if let Some(u) = self.$i.webusb_url(index) {
                    return Some(u);
                })+
                None
            }
        }
    };
}

tuple_handler!(A 0);
tuple_handler!(A 0, B 1);
tuple_handler!(A 0, B 1, C 2);
tuple_handler!(A 0, B 1, C 2, D 3);
tuple_handler!(A 0, B 1, C 2, D 3, E 4);
tuple_handler!(A 0, B 1, C 2, D 3, E 4, F 5);

/// Where the bytes of an IN data stage come from.
#[derive(Copy, Clone, Debug)]
enum Source {
    Static(&'static [u8]),
    /// A string descriptor of a string known at build time, encoded packet by packet.
    Str(&'static str),
    /// A string descriptor of [Handler::string] `index`.
    Runtime(u8),
    /// A URL descriptor of [Handler::webusb_url] `index`.
    Url(u8),
    /// GET_STATUS, GET_CONFIGURATION, GET_INTERFACE.
    Inline([u8; 2]),
    /// [Control]'s buffer, filled by [Handler::control_in].
    Buffer,
}

#[derive(Copy, Clone, Debug)]
enum Stage {
    Idle,
    DataIn {
        source: Source,
        offset: u16,
        total: u16,
    },
    StatusOut,
    DataOut {
        received: u16,
    },
    StatusIn {
        address: Option<u8>,
    },
}

/// How a request is answered, before the data stage is set up.
enum Reply {
    Stall,
    In(Source, usize),
    /// No data stage: the status stage completes with this effect.
    Status(Effect),
    Address(u8),
    /// An OUT data stage into the buffer.
    Out,
}

/// The EP0 state machine of a device `D`. `N` is the buffer for class requests' data stages (IN data a handler
/// writes, OUT data it gets), 64 bytes unless the firmware asks for more; an OUT request with a longer data stage
/// is stalled.
pub struct Control<D: Descriptors, const N: usize = 64> {
    state: DeviceState,
    address: u8,
    remote_wakeup: bool,
    alt: [u8; MAX_INTERFACES],
    /// Bit n: OUT endpoint n halted, bit 16 + n: IN endpoint n.
    halted: u32,
    setup: Setup,
    stage: Stage,
    buf: [u8; N],
    _device: PhantomData<D>,
}

impl<D: Descriptors, const N: usize> Default for Control<D, N> {
    fn default() -> Self {
        Self::new()
    }
}

impl<D: Descriptors, const N: usize> Control<D, N> {
    const MPS: usize = D::DEVICE.max_packet_size_0 as usize;

    pub const fn new() -> Self {
        const {
            assert!(
                D::DEVICE.interface_count() as usize <= MAX_INTERFACES,
                "bedrock_usb::control serves at most MAX_INTERFACES interfaces"
            )
        };
        Control {
            state: DeviceState::Default,
            address: 0,
            remote_wakeup: false,
            alt: [0; MAX_INTERFACES],
            halted: 0,
            setup: Setup {
                request_type: 0,
                request: 0,
                value: 0,
                index: 0,
                length: 0,
            },
            stage: Stage::Idle,
            buf: [0; N],
            _device: PhantomData,
        }
    }

    pub fn state(&self) -> DeviceState {
        self.state
    }

    /// The device address, from the end of SET_ADDRESS's status stage.
    pub fn address(&self) -> u8 {
        self.address
    }

    pub fn configured(&self) -> bool {
        self.state == DeviceState::Configured
    }

    /// Whether the host enabled remote wakeup (SET_FEATURE(DEVICE_REMOTE_WAKEUP)).
    pub fn remote_wakeup_enabled(&self) -> bool {
        self.remote_wakeup
    }

    /// The current alternate setting of `interface` (0 for an interface the device doesn't have).
    pub fn alt_setting(&self, interface: u8) -> u8 {
        self.alt.get(interface as usize).copied().unwrap_or(0)
    }

    /// Whether `endpoint` belongs to the current alternate setting of its interface in the configured state: the
    /// endpoints the driver keeps enabled. Endpoint 0 is always active.
    pub fn is_active(&self, endpoint: EndpointAddress) -> bool {
        if endpoint.number() == 0 {
            return true;
        }
        if self.state != DeviceState::Configured {
            return false;
        }
        let mut it = Interfaces::new(&D::DEVICE);
        while let Some(at) = it.next() {
            if at.interface.alt_setting != self.alt_setting(at.number) {
                continue;
            }
            for item in at.interface.items {
                if let desc::Item::Endpoint(ep) = item
                    && ep.address == endpoint
                {
                    return true;
                }
            }
        }
        false
    }

    /// Whether `endpoint` has the halt feature, as GET_STATUS reports it.
    pub fn is_halted(&self, endpoint: EndpointAddress) -> bool {
        self.halted & halt_bit(endpoint) != 0
    }

    /// Records a halt the class set or cleared itself (a bulk-only mass storage device stalling its IN endpoint),
    /// so GET_STATUS reports it; the driver stalls the endpoint.
    pub fn set_halted(&mut self, endpoint: EndpointAddress, halted: bool) {
        if halted {
            self.halted |= halt_bit(endpoint);
        } else {
            self.halted &= !halt_bit(endpoint);
        }
    }

    /// A bus reset: the default state, address 0, not configured, no halts, remote wakeup off. The driver
    /// re-enables EP0 itself (within 10 ms, RM0444 §37.5.2); a transfer under way is dropped.
    pub fn reset<H: Handler>(&mut self, handler: &mut H) {
        self.state = DeviceState::Default;
        self.address = 0;
        self.remote_wakeup = false;
        self.alt = [0; MAX_INTERFACES];
        self.halted = 0;
        self.stage = Stage::Idle;
        handler.reset();
    }

    /// A SETUP packet arrived. It ends whatever transfer was under way (USB 2.0 §8.5.3: a SETUP is always taken).
    pub fn setup<H: Handler>(&mut self, setup: Setup, handler: &mut H) -> Action {
        self.setup = setup;
        self.stage = Stage::Idle;
        let reply = match setup.kind() {
            RequestKind::Standard => self.standard(handler),
            RequestKind::Class | RequestKind::Vendor => self.class(handler),
            RequestKind::Reserved => Reply::Stall,
        };
        self.start(reply)
    }

    /// An IN packet on EP0 was acknowledged by the host.
    pub fn in_complete(&mut self) -> Action {
        match self.stage {
            Stage::DataIn {
                source,
                offset,
                total,
            } => {
                if self.in_last(offset, total) {
                    self.stage = Stage::StatusOut;
                    Action::ep0(Ep0::ReceiveStatus)
                } else {
                    let offset = offset + Self::MPS as u16;
                    self.stage = Stage::DataIn {
                        source,
                        offset,
                        total,
                    };
                    Action::ep0(Ep0::Send {
                        last: self.in_last(offset, total),
                    })
                }
            }
            Stage::StatusIn { address } => {
                self.stage = Stage::Idle;
                match address {
                    Some(a) => {
                        self.address = a;
                        if a == 0 {
                            self.state = DeviceState::Default;
                        } else if self.state == DeviceState::Default {
                            self.state = DeviceState::Address;
                        }
                        Action {
                            ep0: Ep0::Idle,
                            effect: Effect::SetAddress(a),
                        }
                    }
                    None => Action::ep0(Ep0::Idle),
                }
            }
            _ => Action::ep0(Ep0::Idle),
        }
    }

    /// An OUT packet on EP0 arrived: data of an OUT data stage, or the zero-length status packet after an IN data
    /// stage.
    pub fn out_data<H: Handler>(&mut self, data: &[u8], handler: &mut H) -> Action {
        match self.stage {
            // the status stage, or the host ending the data stage early (USB 2.0 §8.5.3.2): the transfer is over
            Stage::StatusOut | Stage::DataIn { .. } => {
                self.stage = Stage::Idle;
                Action::ep0(Ep0::Idle)
            }
            Stage::DataOut { received } => {
                let start = received as usize;
                let length = self.setup.length as usize;
                let end = start + data.len();
                if end > length {
                    self.stage = Stage::Idle;
                    return Action::ep0(Ep0::Stall);
                }
                self.buf[start..end].copy_from_slice(data);
                if end == length || data.len() < Self::MPS {
                    let setup = self.setup;
                    match handler.control_out(&setup, &self.buf[..end]) {
                        OutResponse::Accept => {
                            self.stage = Stage::StatusIn { address: None };
                            Action::ep0(Ep0::SendStatus)
                        }
                        OutResponse::Reject | OutResponse::Unhandled => {
                            self.stage = Stage::Idle;
                            Action::ep0(Ep0::Stall)
                        }
                    }
                } else {
                    self.stage = Stage::DataOut {
                        received: end as u16,
                    };
                    Action::ep0(Ep0::Receive {
                        last: length - end <= Self::MPS,
                    })
                }
            }
            // an OUT packet nobody asked for
            Stage::Idle | Stage::StatusIn { .. } => {
                self.stage = Stage::Idle;
                Action::ep0(Ep0::Stall)
            }
        }
    }

    /// The next IN data packet after an [Ep0::Send]: written to the start of `out` (at least the EP0 packet size),
    /// its length returned; 0 is a zero-length packet. The same until [Control::in_complete], so a driver can
    /// write it again.
    pub fn packet<H: Handler>(&self, handler: &H, out: &mut [u8]) -> usize {
        let Stage::DataIn {
            source,
            offset,
            total,
        } = self.stage
        else {
            return 0;
        };
        let offset = offset as usize;
        let len = Self::MPS.min(total as usize - offset).min(out.len());
        let out = &mut out[..len];
        match source {
            Source::Static(b) => out.copy_from_slice(&b[offset..offset + len]),
            Source::Buffer => out.copy_from_slice(&self.buf[offset..offset + len]),
            Source::Inline(b) => out.copy_from_slice(&b[offset..offset + len]),
            Source::Str(s) => string_bytes(s, offset, out),
            Source::Runtime(i) => match handler.string(i) {
                Some(s) => string_bytes(s, offset, out),
                None => out.fill(0),
            },
            Source::Url(i) => match handler.webusb_url(i) {
                Some(u) => url_bytes(&u, offset, out),
                None => out.fill(0),
            },
        }
        len
    }

    /// Whether the packet at `offset` ends the IN data stage (USB 2.0 §5.5.3).
    fn in_last(&self, offset: u16, total: u16) -> bool {
        let len = Self::MPS.min((total - offset) as usize);
        offset as usize + len == total as usize && (len < Self::MPS || total == self.setup.length)
    }

    fn start(&mut self, reply: Reply) -> Action {
        let length = self.setup.length as usize;
        match reply {
            Reply::Stall => Action::ep0(Ep0::Stall),
            // a request without data stage (wLength 0) that would have answered: the status stage alone
            Reply::In(_, _) if length == 0 => {
                self.stage = Stage::StatusIn { address: None };
                Action::ep0(Ep0::SendStatus)
            }
            Reply::In(source, available) => {
                let total = available.min(length) as u16;
                self.stage = Stage::DataIn {
                    source,
                    offset: 0,
                    total,
                };
                Action::ep0(Ep0::Send {
                    last: self.in_last(0, total),
                })
            }
            Reply::Status(effect) => {
                self.stage = Stage::StatusIn { address: None };
                Action {
                    ep0: Ep0::SendStatus,
                    effect,
                }
            }
            Reply::Address(a) => {
                self.stage = Stage::StatusIn { address: Some(a) };
                Action::ep0(Ep0::SendStatus)
            }
            Reply::Out => {
                self.stage = Stage::DataOut { received: 0 };
                Action::ep0(Ep0::Receive {
                    last: length <= Self::MPS,
                })
            }
        }
    }

    fn standard<H: Handler>(&mut self, handler: &mut H) -> Reply {
        let s = self.setup;
        let dir_in = s.direction() == Direction::In;
        // a request without data stage: OUT, wLength 0
        let no_data = !dir_in && s.length == 0;
        match (s.recipient(), s.request) {
            (Recipient::Device, request::GET_STATUS) if dir_in && s.value == 0 => {
                let status = D::DEVICE.self_powered as u8 | ((self.remote_wakeup as u8) << 1);
                Reply::In(Source::Inline([status, 0]), 2)
            }
            (Recipient::Device, request::CLEAR_FEATURE | request::SET_FEATURE)
                if no_data
                    && s.value == request::FEATURE_DEVICE_REMOTE_WAKEUP
                    && D::DEVICE.remote_wakeup =>
            {
                self.remote_wakeup = s.request == request::SET_FEATURE;
                Reply::Status(Effect::None)
            }
            (Recipient::Device, request::SET_ADDRESS)
                if no_data && s.value <= 127 && s.index == 0 =>
            {
                Reply::Address(s.value as u8)
            }
            (Recipient::Device, request::GET_DESCRIPTOR) if dir_in => self.descriptor(handler),
            (Recipient::Device, request::GET_CONFIGURATION) if dir_in => {
                let value = if self.configured() {
                    CONFIGURATION_VALUE
                } else {
                    0
                };
                Reply::In(Source::Inline([value, 0]), 1)
            }
            (Recipient::Device, request::SET_CONFIGURATION)
                if no_data && self.state != DeviceState::Default =>
            {
                let configured = match s.value {
                    0 => false,
                    v if v == CONFIGURATION_VALUE as u16 => true,
                    _ => return Reply::Stall,
                };
                self.state = if configured {
                    DeviceState::Configured
                } else {
                    DeviceState::Address
                };
                self.alt = [0; MAX_INTERFACES];
                self.halted = 0;
                handler.configured(configured);
                Reply::Status(Effect::Configure(configured))
            }
            (Recipient::Interface, request::GET_STATUS)
                if dir_in && s.value == 0 && self.interface(s.index).is_some() =>
            {
                Reply::In(Source::Inline([0, 0]), 2)
            }
            (Recipient::Interface, request::GET_INTERFACE) if dir_in && s.value == 0 => {
                match self.interface(s.index) {
                    Some(i) => Reply::In(Source::Inline([self.alt[i as usize], 0]), 1),
                    None => Reply::Stall,
                }
            }
            (Recipient::Interface, request::SET_INTERFACE) if no_data => {
                let Some(interface) = self.interface(s.index) else {
                    return Reply::Stall;
                };
                if s.value >= D::DEVICE.alt_settings(interface) as u16 {
                    return Reply::Stall;
                }
                let alt = s.value as u8;
                self.alt[interface as usize] = alt;
                let mut it = Endpoints::new(&D::DEVICE);
                while let Some((i, ep)) = it.next() {
                    if i == interface {
                        self.set_halted(ep.address, false);
                    }
                }
                handler.set_alternate_setting(interface, alt);
                Reply::Status(Effect::SetInterface { interface, alt })
            }
            // the interface's own standard requests (a HID report descriptor): its class handler's
            (Recipient::Interface, _) => self.class(handler),
            (Recipient::Endpoint, request::GET_STATUS) if dir_in && s.value == 0 => {
                match self.endpoint(s.index) {
                    Some(ep) => Reply::In(Source::Inline([self.is_halted(ep) as u8, 0]), 2),
                    None => Reply::Stall,
                }
            }
            (Recipient::Endpoint, request::CLEAR_FEATURE | request::SET_FEATURE)
                if no_data && s.value == request::FEATURE_ENDPOINT_HALT =>
            {
                let Some(endpoint) = self.endpoint(s.index) else {
                    return Reply::Stall;
                };
                let halted = s.request == request::SET_FEATURE;
                if endpoint.number() == 0 {
                    // no halt feature on the default control pipe (USB 2.0 §9.4.5: neither required nor
                    // recommended); clearing it is harmless
                    return if halted {
                        Reply::Stall
                    } else {
                        Reply::Status(Effect::None)
                    };
                }
                self.set_halted(endpoint, halted);
                Reply::Status(Effect::Halt { endpoint, halted })
            }
            _ => Reply::Stall,
        }
    }

    fn descriptor<H: Handler>(&self, handler: &H) -> Reply {
        let s = self.setup;
        match (s.descriptor_type(), s.descriptor_index()) {
            (descriptor_type::DEVICE, 0) => {
                let d: &'static [u8] = const { &D::DEVICE_DESC };
                Reply::In(Source::Static(d), d.len())
            }
            (descriptor_type::CONFIGURATION, 0) => {
                Reply::In(Source::Static(D::CONFIG_DESC), D::CONFIG_DESC.len())
            }
            (descriptor_type::BOS, 0) if !D::BOS.is_empty() => {
                Reply::In(Source::Static(D::BOS), D::BOS.len())
            }
            // the language ID of every string, whatever wIndex holds
            (descriptor_type::STRING, 0) => {
                Reply::In(Source::Static(&desc::STRING0), desc::STRING0.len())
            }
            (descriptor_type::STRING, index) => match D::DEVICE.string(index) {
                Some(Str::Const(text)) => Reply::In(Source::Str(text), string_len(text)),
                Some(Str::Runtime) => match handler.string(index) {
                    Some(text) => Reply::In(Source::Runtime(index), string_len(text)),
                    None => Reply::Stall,
                },
                _ => Reply::Stall,
            },
            // DEVICE_QUALIFIER and OTHER_SPEED_CONFIGURATION: a full-speed-only device has none (USB 2.0 §9.6.2)
            _ => Reply::Stall,
        }
    }

    /// A class or vendor request, or an interface's standard request the machine doesn't serve.
    fn class<H: Handler>(&mut self, handler: &mut H) -> Reply {
        let s = self.setup;
        let dir_in = s.direction() == Direction::In;
        if dir_in && s.kind() == RequestKind::Vendor && s.recipient() == Recipient::Device {
            if let Some(ms) = D::DEVICE.ms_os_20
                && s.request == ms.vendor_code
                && s.index == request::MS_OS_20_DESCRIPTOR_INDEX
            {
                return Reply::In(Source::Static(D::MS_OS_20_SET), D::MS_OS_20_SET.len());
            }
            if let Some(w) = D::DEVICE.webusb
                && s.request == w.vendor_code
                && s.index == request::WEBUSB_GET_URL
            {
                let index = s.value as u8;
                return match handler.webusb_url(index) {
                    Some(u) if s.value <= 0xff => Reply::In(Source::Url(index), url_len(&u)),
                    _ => Reply::Stall,
                };
            }
        }
        if dir_in {
            match handler.control_in(&s, &mut self.buf) {
                InResponse::Static(b) => Reply::In(Source::Static(b), b.len()),
                InResponse::Data(n) => Reply::In(Source::Buffer, n.min(N)),
                InResponse::Reject | InResponse::Unhandled => Reply::Stall,
            }
        } else if s.length == 0 {
            match handler.control_out(&s, &[]) {
                OutResponse::Accept => Reply::Status(Effect::None),
                OutResponse::Reject | OutResponse::Unhandled => Reply::Stall,
            }
        } else if s.length as usize <= N {
            Reply::Out
        } else {
            Reply::Stall
        }
    }

    /// The interface `wIndex` names, if the device is configured and has it.
    fn interface(&self, index: u16) -> Option<u8> {
        (self.configured() && index < D::DEVICE.interface_count() as u16).then_some(index as u8)
    }

    /// The endpoint `wIndex` names, if it may be addressed now: endpoint 0 always, a class endpoint in the
    /// configured state when it is in its interface's current alternate setting (USB 2.0 §9.4: a request error
    /// otherwise).
    fn endpoint(&self, index: u16) -> Option<EndpointAddress> {
        if index > 0xff || index & 0x70 != 0 {
            return None;
        }
        let ep = EndpointAddress::from_raw(index as u8);
        self.is_active(ep).then_some(ep)
    }
}

const fn halt_bit(endpoint: EndpointAddress) -> u32 {
    let shift = match endpoint.direction() {
        Direction::Out => 0,
        Direction::In => 16,
    };
    1 << (endpoint.number() + shift)
}

/// Writes the bytes at `offset..offset + out.len()` of a descriptor generated byte by byte.
struct Window<'a> {
    skip: usize,
    out: &'a mut [u8],
    at: usize,
}

impl Window<'_> {
    fn push(&mut self, b: u8) {
        if self.skip > 0 {
            self.skip -= 1;
        } else if self.at < self.out.len() {
            self.out[self.at] = b;
            self.at += 1;
        }
    }
}

/// UTF-16 code units of a string descriptor's text that are sent: at most 126, so `bLength` fits a byte.
fn string_units(text: &str) -> usize {
    text.encode_utf16().count().min(126)
}

fn string_len(text: &str) -> usize {
    2 + 2 * string_units(text)
}

fn string_bytes(text: &str, offset: usize, out: &mut [u8]) {
    let mut w = Window {
        skip: offset,
        out,
        at: 0,
    };
    w.push(string_len(text) as u8);
    w.push(descriptor_type::STRING);
    for unit in text.encode_utf16().take(126) {
        let [lo, hi] = unit.to_le_bytes();
        w.push(lo);
        w.push(hi);
        if w.at == w.out.len() {
            break;
        }
    }
}

fn url_len(url: &Url<'_>) -> usize {
    3 + url.url.len().min(252)
}

fn url_bytes(url: &Url<'_>, offset: usize, out: &mut [u8]) {
    let mut w = Window {
        skip: offset,
        out,
        at: 0,
    };
    w.push(url_len(url) as u8);
    w.push(WEBUSB_URL);
    w.push(url.scheme as u8);
    for &b in url.url.as_bytes().iter().take(252) {
        w.push(b);
    }
}
