//! Chapter 9 cases against the EP0 control machine, driven by a simulated host.
//!
//! [transfer] runs one control transfer the way a host does: the SETUP, then IN packets until a short one or
//! `wLength` bytes (USB 2.0 §5.5.3), OUT packets of the EP0 packet size, the status stage. At each packet it checks
//! that the machine's `last` (how the driver arms the other direction) agrees with the host's own view of where the
//! data stage ends, and collects the bytes, the packet sizes, the effects and whether the transfer stalled.

use bedrock_usb::Buffering::{Double, Single};
use bedrock_usb::control::{
    Control, DeviceState, Effect, Ep0, Handler, InResponse, OutResponse, Url, UrlScheme,
};
use bedrock_usb::desc::{
    self, Device, DeviceClass, Endpoint, Function, Interface, Item, MsOs20, MsOsFeature, STRING0,
    Str, WINDOWS_8_1, WebUsb, string_descriptor, string_len,
};
use bedrock_usb::{Descriptors, Direction, EndpointAddress, Setup, descriptor_type, request};

// --- devices ---

const PRODUCT: &str = "Grüße 🦀";
const ALT_STRING: &str = "alt one";

const DEV: Device = Device {
    manufacturer: Str::Const("bedrock"),
    product: Str::Const(PRODUCT),
    serial: Str::Runtime,
    self_powered: true,
    remote_wakeup: true,
    ms_os_20: Some(MsOs20 {
        vendor_code: 0x01,
        windows_version: WINDOWS_8_1,
        device: &[MsOsFeature::CompatibleId {
            id: "WINUSB",
            sub_id: "",
        }],
    }),
    webusb: Some(WebUsb {
        vendor_code: 0x02,
        landing_page: 1,
    }),
    functions: &[
        Function {
            interfaces: &[Interface {
                string: Str::Runtime, // index 4
                items: &[
                    Item::Endpoint(Endpoint::bulk_out(1, 64, Single)),
                    Item::Endpoint(Endpoint::bulk_in(1, 64, Single)),
                    Item::Endpoint(Endpoint::bulk_in(3, 64, Double)),
                ],
                ..Interface::vendor()
            }],
            ..Function::vendor()
        },
        Function {
            interfaces: &[
                Interface::vendor(), // interface 1, alt 0: no endpoints
                Interface {
                    alt_setting: 1,
                    string: Str::Const(ALT_STRING), // index 5
                    items: &[Item::Endpoint(Endpoint::interrupt_in(2, 8, 10))],
                    ..Interface::vendor()
                },
            ],
            ..Function::vendor()
        },
    ],
    ..Device::new(0xc0de, 0xcafe)
};
bedrock_usb::descriptors!(Dev = DEV);

/// A configuration descriptor of exactly 64 bytes: 9 + 9 + 7 + 7 + a 32-byte class descriptor.
const PAD: [u8; 30] = [0x5a; 30];
const DEV64: Device = Device {
    class: DeviceClass::PerInterface,
    usb_version: 0x0200,
    functions: &[Function {
        interfaces: &[Interface {
            items: &[
                Item::Endpoint(Endpoint::bulk_out(1, 64, Single)),
                Item::Endpoint(Endpoint::bulk_in(1, 64, Single)),
                Item::Class {
                    kind: 0x24,
                    data: &PAD,
                },
            ],
            ..Interface::vendor()
        }],
        ..Function::vendor()
    }],
    ..Device::new(0xc0de, 0x0064)
};
bedrock_usb::descriptors!(Dev64 = DEV64);
const _: () = assert!(Dev64::CONFIG_DESC.len() == 64);

/// The same with an 8-byte EP0: multi-packet stages everywhere.
const DEV8: Device = Device {
    max_packet_size_0: 8,
    ..DEV64
};
bedrock_usb::descriptors!(Dev8 = DEV8);

// --- handlers ---

const SERIAL: &str = "0123456789AB";
const BIG: [u8; 130] = {
    let mut b = [0u8; 130];
    let mut i = 0;
    while i < 130 {
        b[i] = i as u8;
        i += 1;
    }
    b
};

const VENDOR_IN: u8 = 0xc1; // vendor, IN, interface
const VENDOR_OUT: u8 = 0x41; // vendor, OUT, interface

#[derive(Default)]
struct Rec {
    events: Vec<String>,
    out: Vec<(u8, Vec<u8>)>,
    serial: Option<&'static str>,
    label: String,
}

impl Rec {
    fn new() -> Self {
        Rec {
            serial: Some(SERIAL),
            label: "label".into(),
            ..Rec::default()
        }
    }
}

impl Handler for Rec {
    fn reset(&mut self) {
        self.events.push("reset".into());
    }
    fn configured(&mut self, configured: bool) {
        self.events.push(format!("configured {configured}"));
    }
    fn set_alternate_setting(&mut self, interface: u8, alt: u8) {
        self.events.push(format!("alt {interface} {alt}"));
    }
    fn control_in(&mut self, setup: &Setup, buf: &mut [u8]) -> InResponse {
        if setup.request_type != VENDOR_IN {
            return InResponse::Unhandled;
        }
        match setup.request {
            // as many bytes as wValue says, 0, 1, 2, ..
            0x10 => {
                let n = setup.value as usize;
                for (i, b) in buf[..n].iter_mut().enumerate() {
                    *b = i as u8;
                }
                InResponse::Data(n)
            }
            0x11 => InResponse::Static(&BIG),
            0x12 => InResponse::Reject,
            _ => InResponse::Unhandled,
        }
    }
    fn control_out(&mut self, setup: &Setup, data: &[u8]) -> OutResponse {
        if setup.request_type != VENDOR_OUT {
            return OutResponse::Unhandled;
        }
        match setup.request {
            0x30 => {
                self.out.push((setup.request, data.to_vec()));
                OutResponse::Accept
            }
            0x31 => OutResponse::Reject,
            _ => OutResponse::Unhandled,
        }
    }
    fn string(&self, index: u8) -> Option<&str> {
        match index {
            desc::STRING_SERIAL => self.serial,
            4 => Some(&self.label),
            _ => None,
        }
    }
    fn webusb_url(&self, index: u8) -> Option<Url<'_>> {
        (index == 1).then_some(Url {
            scheme: UrlScheme::Https,
            url: "example.com/dev",
        })
    }
}

/// A second class in the tuple: takes vendor request 0x40 both ways, nothing the first one takes.
#[derive(Default)]
struct Second {
    resets: u32,
}

impl Handler for Second {
    fn reset(&mut self) {
        self.resets += 1;
    }
    fn control_in(&mut self, setup: &Setup, buf: &mut [u8]) -> InResponse {
        if setup.request == 0x40 || setup.request == 0x10 {
            buf[0] = 0x99;
            return InResponse::Data(1);
        }
        InResponse::Unhandled
    }
    fn control_out(&mut self, setup: &Setup, _data: &[u8]) -> OutResponse {
        if setup.request == 0x40 {
            OutResponse::Accept
        } else {
            OutResponse::Unhandled
        }
    }
}

// --- the simulated host ---

#[derive(Debug, Default)]
struct Transfer {
    data: Vec<u8>,
    packets: Vec<usize>,
    stalled: bool,
    status_in: bool,
    effects: Vec<Effect>,
}

fn setup(request_type: u8, request: u8, value: u16, index: u16, length: u16) -> Setup {
    Setup {
        request_type,
        request,
        value,
        index,
        length,
    }
}

fn get_descriptor(kind: u8, index: u8, length: u16) -> Setup {
    setup(
        0x80,
        request::GET_DESCRIPTOR,
        (kind as u16) << 8 | index as u16,
        0,
        length,
    )
}

/// One control transfer; `out` is the OUT data stage (its length is `wLength` of an OUT request).
fn transfer<D: Descriptors, const N: usize, H: Handler>(
    c: &mut Control<D, N>,
    h: &mut H,
    s: Setup,
    out: &[u8],
) -> Transfer {
    let mps = D::DEVICE.max_packet_size_0 as usize;
    if s.direction() == Direction::Out {
        assert_eq!(out.len(), s.length as usize, "test: OUT data is wLength");
    }
    let mut t = Transfer::default();
    let mut sent = 0;
    let mut a = c.setup(s, h);
    for _ in 0..1000 {
        if a.effect != Effect::None {
            t.effects.push(a.effect);
        }
        match a.ep0 {
            Ep0::Idle => return t,
            Ep0::Stall => {
                t.stalled = true;
                return t;
            }
            Ep0::Send { last } => {
                assert_eq!(s.direction(), Direction::In, "IN data on an OUT request");
                let mut buf = [0xeeu8; 64];
                let n = c.packet(h, &mut buf);
                let mut again = [0u8; 64];
                assert_eq!(c.packet(h, &mut again), n, "packet() is repeatable");
                assert_eq!(buf[..n], again[..n]);
                assert!(n <= mps, "packet over bMaxPacketSize0");
                t.data.extend_from_slice(&buf[..n]);
                t.packets.push(n);
                assert!(t.data.len() <= s.length as usize, "more than wLength");
                let host_done = n < mps || t.data.len() == s.length as usize;
                assert_eq!(
                    last, host_done,
                    "`last` disagrees with the host after {t:?}"
                );
                a = c.in_complete();
            }
            Ep0::ReceiveStatus => {
                assert_eq!(s.direction(), Direction::In);
                a = c.out_data(&[], h);
                assert_eq!(a.ep0, Ep0::Idle, "status OUT ends the transfer");
            }
            Ep0::Receive { last } => {
                assert_eq!(s.direction(), Direction::Out, "OUT data on an IN request");
                let n = mps.min(out.len() - sent);
                assert_eq!(last, out.len() - sent <= mps, "`last` of the OUT stage");
                let chunk = &out[sent..sent + n];
                sent += n;
                a = c.out_data(chunk, h);
            }
            Ep0::SendStatus => {
                assert!(
                    s.length == 0 || s.direction() == Direction::Out,
                    "IN status after an IN data stage"
                );
                t.status_in = true;
                a = c.in_complete();
                if a.effect != Effect::None {
                    t.effects.push(a.effect);
                }
                assert_eq!(a.ep0, Ep0::Idle, "status IN ends the transfer");
                return t;
            }
        }
    }
    panic!("transfer doesn't end");
}

fn ok_in<D: Descriptors, const N: usize, H: Handler>(
    c: &mut Control<D, N>,
    h: &mut H,
    s: Setup,
) -> Vec<u8> {
    let t = transfer(c, h, s, &[]);
    assert!(!t.stalled, "{s:?} stalled");
    t.data
}

fn stalls<D: Descriptors, const N: usize, H: Handler>(
    c: &mut Control<D, N>,
    h: &mut H,
    s: Setup,
) -> bool {
    transfer(c, h, s, &[]).stalled
}

fn set_address(a: u16) -> Setup {
    setup(0x00, request::SET_ADDRESS, a, 0, 0)
}

fn set_configuration(v: u16) -> Setup {
    setup(0x00, request::SET_CONFIGURATION, v, 0, 0)
}

/// Address 5 and configuration 1, as after enumeration.
fn configured<D: Descriptors>() -> (Control<D>, Rec) {
    let mut c = Control::<D>::new();
    let mut h = Rec::new();
    c.reset(&mut h);
    assert!(!transfer(&mut c, &mut h, set_address(5), &[]).stalled);
    assert!(!transfer(&mut c, &mut h, set_configuration(1), &[]).stalled);
    assert_eq!(c.state(), DeviceState::Configured);
    h.events.clear();
    (c, h)
}

const fn ep(number: u8, direction: Direction) -> EndpointAddress {
    EndpointAddress::new(number, direction)
}

// --- GET_DESCRIPTOR and wLength (R13) ---

#[test]
fn device_descriptor_around_wlength() {
    let mut c = Control::<Dev>::new();
    let mut h = Rec::new();
    // Windows' first request: 64 bytes asked, 18 come back in one short packet
    let t = transfer(
        &mut c,
        &mut h,
        get_descriptor(descriptor_type::DEVICE, 0, 64),
        &[],
    );
    assert_eq!(t.data, Dev::DEVICE_DESC);
    assert_eq!(t.packets, [18]);
    // Linux on some hubs: only the first 8
    let t = transfer(
        &mut c,
        &mut h,
        get_descriptor(descriptor_type::DEVICE, 0, 8),
        &[],
    );
    assert_eq!(t.data, Dev::DEVICE_DESC[..8]);
    assert_eq!(t.packets, [8]);
    // exactly 18
    let t = transfer(
        &mut c,
        &mut h,
        get_descriptor(descriptor_type::DEVICE, 0, 18),
        &[],
    );
    assert_eq!(t.packets, [18]);
    // wLength 0: no data stage, the IN status alone
    let t = transfer(
        &mut c,
        &mut h,
        get_descriptor(descriptor_type::DEVICE, 0, 0),
        &[],
    );
    assert!(!t.stalled && t.status_in && t.data.is_empty());
}

#[test]
fn configuration_at_a_multiple_of_64() {
    let mut c = Control::<Dev64>::new();
    let mut h = ();
    let cfg = |len| get_descriptor(descriptor_type::CONFIGURATION, 0, len);
    // shorter than the descriptor: wLength ends it, no ZLP
    let t = transfer(&mut c, &mut h, cfg(63), &[]);
    assert_eq!(
        (t.data.as_slice(), t.packets.as_slice()),
        (&Dev64::CONFIG_DESC[..63], &[63][..])
    );
    // equal: one full packet and no ZLP
    let t = transfer(&mut c, &mut h, cfg(64), &[]);
    assert_eq!(
        (t.data.as_slice(), t.packets.as_slice()),
        (&Dev64::CONFIG_DESC[..], &[64][..])
    );
    // longer: the full packet doesn't end the stage, a ZLP does
    for len in [65, 128, 255, 0xffff] {
        let t = transfer(&mut c, &mut h, cfg(len), &[]);
        assert_eq!(t.data, Dev64::CONFIG_DESC);
        assert_eq!(t.packets, [64, 0], "wLength {len}");
    }
    // the header alone, as hosts read it first
    let t = transfer(&mut c, &mut h, cfg(9), &[]);
    assert_eq!(t.data, Dev64::CONFIG_DESC[..9]);
}

#[test]
fn small_ep0_packets() {
    let mut c = Control::<Dev8>::new();
    let mut h = ();
    let cfg = |len| get_descriptor(descriptor_type::CONFIGURATION, 0, len);
    let t = transfer(&mut c, &mut h, cfg(255), &[]);
    assert_eq!(t.data, Dev8::CONFIG_DESC);
    assert_eq!(
        t.packets,
        [8; 8].iter().copied().chain([0]).collect::<Vec<_>>()
    );
    let t = transfer(&mut c, &mut h, cfg(64), &[]);
    assert_eq!(t.packets, [8; 8]);
    let t = transfer(&mut c, &mut h, cfg(20), &[]);
    assert_eq!(t.packets, [8, 8, 4]);
    let t = transfer(
        &mut c,
        &mut h,
        get_descriptor(descriptor_type::DEVICE, 0, 255),
        &[],
    );
    assert_eq!(t.data, Dev8::DEVICE_DESC);
    assert_eq!(t.packets, [8, 8, 2]);
}

#[test]
fn configuration_and_bos_of_a_composite_device() {
    let mut c = Control::<Dev>::new();
    let mut h = Rec::new();
    let t = transfer(
        &mut c,
        &mut h,
        get_descriptor(descriptor_type::CONFIGURATION, 0, 0xff),
        &[],
    );
    assert_eq!(t.data, Dev::CONFIG_DESC);
    assert_eq!(*t.packets.last().unwrap(), Dev::CONFIG_DESC.len() % 64);
    let bos = ok_in(
        &mut c,
        &mut h,
        get_descriptor(descriptor_type::BOS, 0, 0xff),
    );
    assert_eq!(bos, Dev::BOS);
    // configuration index 1: there is one configuration
    assert!(stalls(
        &mut c,
        &mut h,
        get_descriptor(descriptor_type::CONFIGURATION, 1, 0xff)
    ));
}

#[test]
fn no_bos_on_usb_2_0() {
    let mut c = Control::<Dev64>::new();
    assert!(stalls(
        &mut c,
        &mut (),
        get_descriptor(descriptor_type::BOS, 0, 0xff)
    ));
}

#[test]
fn device_qualifier_and_other_speed_stall() {
    let mut c = Control::<Dev>::new();
    let mut h = Rec::new();
    assert!(stalls(
        &mut c,
        &mut h,
        get_descriptor(descriptor_type::DEVICE_QUALIFIER, 0, 10)
    ));
    assert!(stalls(
        &mut c,
        &mut h,
        get_descriptor(descriptor_type::OTHER_SPEED_CONFIGURATION, 0, 0xff)
    ));
    // a stall ends that transfer only: the next request is answered
    assert_eq!(
        ok_in(
            &mut c,
            &mut h,
            get_descriptor(descriptor_type::DEVICE, 0, 18)
        ),
        Dev::DEVICE_DESC
    );
}

// --- strings ---

#[test]
fn string_zero_and_language() {
    let mut c = Control::<Dev>::new();
    let mut h = Rec::new();
    assert_eq!(
        ok_in(
            &mut c,
            &mut h,
            get_descriptor(descriptor_type::STRING, 0, 0xff)
        ),
        STRING0
    );
    // the length first, as Windows reads it
    assert_eq!(
        ok_in(
            &mut c,
            &mut h,
            get_descriptor(descriptor_type::STRING, 0, 2)
        ),
        STRING0[..2]
    );
}

#[test]
fn const_strings_encode_like_desc() {
    let mut c = Control::<Dev>::new();
    let mut h = Rec::new();
    let mut s = get_descriptor(descriptor_type::STRING, desc::STRING_PRODUCT, 0xff);
    s.index = desc::LANG_ID_EN_US;
    let want: [u8; string_len(PRODUCT)] = string_descriptor(PRODUCT);
    assert_eq!(
        ok_in(&mut c, &mut h, s),
        want,
        "non-ASCII and a surrogate pair"
    );
    let want: [u8; string_len("bedrock")] = string_descriptor("bedrock");
    let s = get_descriptor(descriptor_type::STRING, desc::STRING_MANUFACTURER, 0xff);
    assert_eq!(ok_in(&mut c, &mut h, s), want);
    let want: [u8; string_len(ALT_STRING)] = string_descriptor(ALT_STRING);
    assert_eq!(
        ok_in(
            &mut c,
            &mut h,
            get_descriptor(descriptor_type::STRING, 5, 0xff)
        ),
        want
    );
    // cut by wLength mid-character
    assert_eq!(
        ok_in(
            &mut c,
            &mut h,
            get_descriptor(descriptor_type::STRING, 5, 5)
        ),
        want[..5]
    );
}

#[test]
fn runtime_strings_from_the_handler() {
    let mut c = Control::<Dev>::new();
    let mut h = Rec::new();
    let want: [u8; string_len(SERIAL)] = string_descriptor(SERIAL);
    let s = get_descriptor(descriptor_type::STRING, desc::STRING_SERIAL, 0xff);
    assert_eq!(ok_in(&mut c, &mut h, s), want);
    // a long runtime string goes over several packets and is cut to 126 code units
    h.label = "x".repeat(200);
    let t = transfer(
        &mut c,
        &mut h,
        get_descriptor(descriptor_type::STRING, 4, 0xffff),
        &[],
    );
    assert_eq!(t.data.len(), 254);
    assert_eq!(t.data[..4], [254, descriptor_type::STRING, b'x', 0]);
    assert_eq!(t.packets, [64, 64, 64, 62]);
    // the handler has none: request error
    h.serial = None;
    assert!(stalls(&mut c, &mut h, s));
    // an index the device doesn't use
    assert!(stalls(
        &mut c,
        &mut h,
        get_descriptor(descriptor_type::STRING, 6, 0xff)
    ));
    // product of a device without one
    let mut c64 = Control::<Dev64>::new();
    assert!(stalls(
        &mut c64,
        &mut (),
        get_descriptor(descriptor_type::STRING, desc::STRING_PRODUCT, 0xff)
    ));
}

// --- vendor descriptors ---

#[test]
fn ms_os_20_descriptor_set() {
    let mut c = Control::<Dev>::new();
    let mut h = Rec::new();
    let s = setup(0xc0, 0x01, 0, request::MS_OS_20_DESCRIPTOR_INDEX, 0xffff);
    assert_eq!(ok_in(&mut c, &mut h, s), Dev::MS_OS_20_SET);
    // as Windows asks: exactly wTotalLength
    let s = setup(0xc0, 0x01, 0, 7, Dev::MS_OS_20_SET.len() as u16);
    assert_eq!(ok_in(&mut c, &mut h, s), Dev::MS_OS_20_SET);
    // another wIndex (8, SET_ALT_ENUMERATION) or vendor code goes to the handlers, which don't take it
    assert!(stalls(&mut c, &mut h, setup(0xc0, 0x01, 0, 8, 0xff)));
    assert!(stalls(&mut c, &mut h, setup(0xc0, 0x05, 0, 7, 0xff)));
    // a device without MS OS 2.0
    assert!(stalls(
        &mut Control::<Dev64>::new(),
        &mut (),
        setup(0xc0, 0x01, 0, 7, 0xff)
    ));
}

#[test]
fn webusb_get_url() {
    let mut c = Control::<Dev>::new();
    let mut h = Rec::new();
    let url = ok_in(
        &mut c,
        &mut h,
        setup(0xc0, 0x02, 1, request::WEBUSB_GET_URL, 0xff),
    );
    let mut want = vec![3 + 15, 3, 1];
    want.extend_from_slice(b"example.com/dev");
    assert_eq!(url, want);
    // an index the handler has no URL for
    assert!(stalls(&mut c, &mut h, setup(0xc0, 0x02, 2, 2, 0xff)));
}

// --- SET_ADDRESS (R14) ---

#[test]
fn set_address_after_the_status_stage() {
    let mut c = Control::<Dev>::new();
    let mut h = Rec::new();
    let a = c.setup(set_address(42), &mut h);
    assert_eq!(a.ep0, Ep0::SendStatus);
    assert_eq!(a.effect, Effect::None, "not before the status stage");
    assert_eq!((c.address(), c.state()), (0, DeviceState::Default));
    let a = c.in_complete();
    assert_eq!(a.effect, Effect::SetAddress(42));
    assert_eq!(a.ep0, Ep0::Idle);
    assert_eq!((c.address(), c.state()), (42, DeviceState::Address));
    // a SETUP before the status IN was acknowledged: the address is not taken
    c.setup(set_address(7), &mut h);
    c.setup(get_descriptor(descriptor_type::DEVICE, 0, 18), &mut h);
    assert_eq!(c.in_complete().effect, Effect::None);
    assert_eq!(c.address(), 42);
    // address 0 is the default state again
    let t = transfer(&mut c, &mut h, set_address(0), &[]);
    assert_eq!(t.effects, [Effect::SetAddress(0)]);
    assert_eq!(c.state(), DeviceState::Default);
    // over 127, with wIndex or with a data stage: request error
    assert!(stalls(&mut c, &mut h, set_address(128)));
    assert!(stalls(
        &mut c,
        &mut h,
        setup(0x00, request::SET_ADDRESS, 3, 1, 0)
    ));
    assert!(
        transfer(
            &mut c,
            &mut h,
            setup(0x00, request::SET_ADDRESS, 3, 0, 1),
            &[0]
        )
        .stalled
    );
}

// --- SET_CONFIGURATION / GET_CONFIGURATION ---

#[test]
fn set_configuration_0_1_2() {
    let mut c = Control::<Dev>::new();
    let mut h = Rec::new();
    let get = setup(0x80, request::GET_CONFIGURATION, 0, 0, 1);
    // in the default state the host has no business configuring
    assert!(stalls(&mut c, &mut h, set_configuration(1)));
    transfer(&mut c, &mut h, set_address(5), &[]);
    assert_eq!(ok_in(&mut c, &mut h, get), [0]);
    let t = transfer(&mut c, &mut h, set_configuration(1), &[]);
    assert_eq!(t.effects, [Effect::Configure(true)]);
    assert_eq!(c.state(), DeviceState::Configured);
    assert_eq!(ok_in(&mut c, &mut h, get), [1]);
    // the same configuration again still resets the endpoints (R12)
    let t = transfer(&mut c, &mut h, set_configuration(1), &[]);
    assert_eq!(t.effects, [Effect::Configure(true)]);
    // a configuration the device doesn't have: request error, state unchanged
    assert!(stalls(&mut c, &mut h, set_configuration(2)));
    assert!(stalls(&mut c, &mut h, set_configuration(0x0101)));
    assert_eq!(c.state(), DeviceState::Configured);
    let t = transfer(&mut c, &mut h, set_configuration(0), &[]);
    assert_eq!(t.effects, [Effect::Configure(false)]);
    assert_eq!(c.state(), DeviceState::Address);
    assert_eq!(ok_in(&mut c, &mut h, get), [0]);
    assert_eq!(
        h.events,
        ["configured true", "configured true", "configured false"]
    );
}

// --- SET_INTERFACE / GET_INTERFACE ---

#[test]
fn set_interface() {
    let (mut c, mut h) = configured::<Dev>();
    let set = |i, alt| setup(0x01, request::SET_INTERFACE, alt, i, 0);
    let get = |i| setup(0x81, request::GET_INTERFACE, 0, i, 1);
    let int_in = ep(2, Direction::In);
    assert!(!c.is_active(int_in), "alt 0 of interface 1 has no endpoint");
    // the current alternate setting
    let t = transfer(&mut c, &mut h, set(0, 0), &[]);
    assert_eq!(
        t.effects,
        [Effect::SetInterface {
            interface: 0,
            alt: 0
        }]
    );
    // the other one of interface 1
    let t = transfer(&mut c, &mut h, set(1, 1), &[]);
    assert_eq!(
        t.effects,
        [Effect::SetInterface {
            interface: 1,
            alt: 1
        }]
    );
    assert_eq!(ok_in(&mut c, &mut h, get(1)), [1]);
    assert_eq!(ok_in(&mut c, &mut h, get(0)), [0]);
    assert!(c.is_active(int_in));
    // an alternate setting or an interface the device doesn't have
    assert!(stalls(&mut c, &mut h, set(1, 2)));
    assert!(stalls(&mut c, &mut h, set(0, 1)));
    assert!(stalls(&mut c, &mut h, set(2, 0)));
    assert!(stalls(&mut c, &mut h, get(2)));
    assert_eq!(c.alt_setting(1), 1);
    assert_eq!(h.events, ["alt 0 0", "alt 1 1"]);
    // SET_CONFIGURATION goes back to alternate setting 0
    transfer(&mut c, &mut h, set_configuration(1), &[]);
    assert_eq!(ok_in(&mut c, &mut h, get(1)), [0]);
    assert!(!c.is_active(int_in));
    // not configured: request error
    transfer(&mut c, &mut h, set_configuration(0), &[]);
    assert!(stalls(&mut c, &mut h, set(1, 1)));
    assert!(stalls(&mut c, &mut h, get(0)));
}

// --- GET_STATUS ---

#[test]
fn get_status() {
    let (mut c, mut h) = configured::<Dev>();
    let device = setup(0x80, request::GET_STATUS, 0, 0, 2);
    assert_eq!(ok_in(&mut c, &mut h, device), [0x01, 0], "self powered");
    let rw = |req| setup(0x00, req, request::FEATURE_DEVICE_REMOTE_WAKEUP, 0, 0);
    assert!(!stalls(&mut c, &mut h, rw(request::SET_FEATURE)));
    assert!(c.remote_wakeup_enabled());
    assert_eq!(
        ok_in(&mut c, &mut h, device),
        [0x03, 0],
        "and remote wakeup"
    );
    assert!(!stalls(&mut c, &mut h, rw(request::CLEAR_FEATURE)));
    assert_eq!(ok_in(&mut c, &mut h, device), [0x01, 0]);
    // interfaces have no status bits
    assert_eq!(
        ok_in(&mut c, &mut h, setup(0x81, request::GET_STATUS, 0, 1, 2)),
        [0, 0]
    );
    assert!(stalls(
        &mut c,
        &mut h,
        setup(0x81, request::GET_STATUS, 0, 2, 2)
    ));
    // endpoints: 0 in both directions, the active ones, not the inactive alt's or unknown ones
    let st = |e: u16| setup(0x82, request::GET_STATUS, 0, e, 2);
    for e in [0x00, 0x80, 0x01, 0x81, 0x83] {
        assert_eq!(ok_in(&mut c, &mut h, st(e)), [0, 0], "endpoint {e:#x}");
    }
    for e in [0x82, 0x02, 0x84, 0x0101, 0x11] {
        assert!(stalls(&mut c, &mut h, st(e)), "endpoint {e:#x}");
    }
    // only the first byte when wLength is 1
    assert_eq!(
        ok_in(&mut c, &mut h, setup(0x80, request::GET_STATUS, 0, 0, 1)),
        [0x01]
    );
}

#[test]
fn remote_wakeup_on_a_device_without_it() {
    let mut c = Control::<Dev64>::new();
    let s = setup(
        0x00,
        request::SET_FEATURE,
        request::FEATURE_DEVICE_REMOTE_WAKEUP,
        0,
        0,
    );
    assert!(stalls(&mut c, &mut (), s));
    // TEST_MODE is high speed's
    assert!(stalls(
        &mut c,
        &mut (),
        setup(0x00, request::SET_FEATURE, 2, 0, 0)
    ));
}

#[test]
fn endpoint_requests_before_configuration() {
    let mut c = Control::<Dev>::new();
    let mut h = Rec::new();
    transfer(&mut c, &mut h, set_address(5), &[]);
    let st = |e: u16| setup(0x82, request::GET_STATUS, 0, e, 2);
    assert_eq!(ok_in(&mut c, &mut h, st(0x80)), [0, 0]);
    // USB 2.0 §9.4.5: in the address state only endpoint 0 may be addressed
    assert!(stalls(&mut c, &mut h, st(0x81)));
    assert!(stalls(
        &mut c,
        &mut h,
        setup(0x02, request::SET_FEATURE, 0, 0x81, 0)
    ));
    assert!(stalls(
        &mut c,
        &mut h,
        setup(0x81, request::GET_STATUS, 0, 0, 2)
    ));
}

// --- ENDPOINT_HALT and the data toggle (R12) ---

#[test]
fn halt_and_toggle_reset() {
    let (mut c, mut h) = configured::<Dev>();
    let halt = |req, e| setup(0x02, req, request::FEATURE_ENDPOINT_HALT, e, 0);
    let st = |e| setup(0x82, request::GET_STATUS, 0, e, 2);
    let bulk_in = ep(1, Direction::In);
    let t = transfer(&mut c, &mut h, halt(request::SET_FEATURE, 0x81), &[]);
    assert_eq!(
        t.effects,
        [Effect::Halt {
            endpoint: bulk_in,
            halted: true
        }]
    );
    assert_eq!(ok_in(&mut c, &mut h, st(0x81)), [1, 0]);
    assert_eq!(
        ok_in(&mut c, &mut h, st(0x01)),
        [0, 0],
        "the OUT endpoint of the same number"
    );
    let t = transfer(&mut c, &mut h, halt(request::CLEAR_FEATURE, 0x81), &[]);
    assert_eq!(
        t.effects,
        [Effect::Halt {
            endpoint: bulk_in,
            halted: false
        }]
    );
    assert_eq!(ok_in(&mut c, &mut h, st(0x81)), [0, 0]);
    // clearing a halt that isn't set still resets the toggle: the effect is there
    let t = transfer(&mut c, &mut h, halt(request::CLEAR_FEATURE, 0x01), &[]);
    assert_eq!(
        t.effects,
        [Effect::Halt {
            endpoint: ep(1, Direction::Out),
            halted: false
        }]
    );
    // SET_CONFIGURATION clears halts
    transfer(&mut c, &mut h, halt(request::SET_FEATURE, 0x83), &[]);
    assert!(c.is_halted(ep(3, Direction::In)));
    transfer(&mut c, &mut h, set_configuration(1), &[]);
    assert_eq!(ok_in(&mut c, &mut h, st(0x83)), [0, 0]);
    // SET_INTERFACE clears the halts of that interface only
    transfer(
        &mut c,
        &mut h,
        setup(0x01, request::SET_INTERFACE, 1, 1, 0),
        &[],
    );
    transfer(&mut c, &mut h, halt(request::SET_FEATURE, 0x82), &[]);
    transfer(&mut c, &mut h, halt(request::SET_FEATURE, 0x81), &[]);
    transfer(
        &mut c,
        &mut h,
        setup(0x01, request::SET_INTERFACE, 1, 1, 0),
        &[],
    );
    assert!(!c.is_halted(ep(2, Direction::In)));
    assert!(c.is_halted(bulk_in));
    // a halt the class set itself shows in GET_STATUS
    c.set_halted(ep(1, Direction::Out), true);
    assert_eq!(ok_in(&mut c, &mut h, st(0x01)), [1, 0]);
    // EP0: clearing is harmless, setting is a request error; other feature selectors too
    assert!(!stalls(&mut c, &mut h, halt(request::CLEAR_FEATURE, 0x00)));
    assert!(stalls(&mut c, &mut h, halt(request::SET_FEATURE, 0x80)));
    assert!(stalls(
        &mut c,
        &mut h,
        setup(0x02, request::SET_FEATURE, 5, 0x81, 0)
    ));
    // an endpoint the current settings don't have
    assert!(stalls(&mut c, &mut h, halt(request::SET_FEATURE, 0x84)));
}

// --- transfers interrupted and odd requests ---

#[test]
fn setup_during_a_data_stage() {
    let mut c = Control::<Dev8>::new();
    let mut h = ();
    let a = c.setup(
        get_descriptor(descriptor_type::CONFIGURATION, 0, 0xff),
        &mut h,
    );
    assert_eq!(a.ep0, Ep0::Send { last: false });
    assert_eq!(c.in_complete().ep0, Ep0::Send { last: false });
    // the host gives up on it and starts another: answered from the start
    let t = transfer(
        &mut c,
        &mut h,
        get_descriptor(descriptor_type::DEVICE, 0, 18),
        &[],
    );
    assert_eq!(t.data, Dev8::DEVICE_DESC);
    // and during an OUT data stage
    let mut c = Control::<Dev, 128>::new();
    let mut h = Rec::new();
    let a = c.setup(setup(VENDOR_OUT, 0x30, 0, 0, 100), &mut h);
    assert_eq!(a.ep0, Ep0::Receive { last: false });
    assert_eq!(
        c.out_data(&[1; 64], &mut h).ep0,
        Ep0::Receive { last: true }
    );
    let t = transfer(&mut c, &mut h, setup(VENDOR_OUT, 0x30, 0, 0, 3), &[7, 8, 9]);
    assert!(t.status_in);
    assert_eq!(
        h.out,
        [(0x30, vec![7, 8, 9])],
        "the broken transfer reached nobody"
    );
}

#[test]
fn host_ends_the_in_stage_early() {
    let mut c = Control::<Dev8>::new();
    c.setup(
        get_descriptor(descriptor_type::CONFIGURATION, 0, 0xff),
        &mut (),
    );
    c.in_complete();
    assert_eq!(c.out_data(&[], &mut ()).ep0, Ep0::Idle);
    // nothing left of it
    assert_eq!(c.in_complete().ep0, Ep0::Idle);
    assert_eq!(c.packet(&(), &mut [0; 8]), 0);
}

#[test]
fn stray_packets_stall() {
    let mut c = Control::<Dev>::new();
    assert_eq!(c.out_data(&[1, 2], &mut Rec::new()).ep0, Ep0::Stall);
    assert_eq!(c.in_complete().ep0, Ep0::Idle);
}

#[test]
fn unknown_requests_stall() {
    let (mut c, mut h) = configured::<Dev>();
    let cases = [
        // SET_DESCRIPTOR is optional, SYNCH_FRAME is for isochronous endpoints
        setup(0x00, request::SET_DESCRIPTOR, 0x0100, 0, 0),
        setup(0x82, request::SYNCH_FRAME, 0, 0x81, 2),
        // a standard request code that doesn't exist
        setup(0x80, 0x42, 0, 0, 0),
        // GET_STATUS with the wrong direction
        setup(0x00, request::GET_STATUS, 0, 0, 0),
        // a reserved request type and recipient
        setup(0xe0, 0x01, 0, 0, 0),
        setup(0x83, request::GET_STATUS, 0, 0, 2),
        // class and vendor requests nobody takes, IN and OUT
        setup(0xa1, 0x01, 0, 0, 8),
        setup(0x21, 0x01, 0, 0, 0),
        setup(VENDOR_IN, 0x7f, 0, 0, 8),
        // an interface's standard request nobody takes (a HID report descriptor)
        get_descriptor(0x22, 0, 64),
    ];
    for s in cases {
        assert!(stalls(&mut c, &mut h, s), "{s:?}");
    }
    let mut s = get_descriptor(0x22, 0, 64);
    s.request_type = 0x81;
    assert!(stalls(&mut c, &mut h, s));
    assert_eq!(c.state(), DeviceState::Configured, "nothing changed");
}

// --- class requests ---

#[test]
fn class_in_requests() {
    let (mut c, mut h) = configured::<Dev>();
    // from the buffer, cut to wLength
    assert_eq!(
        ok_in(&mut c, &mut h, setup(VENDOR_IN, 0x10, 5, 0, 64)),
        [0, 1, 2, 3, 4]
    );
    assert_eq!(
        ok_in(&mut c, &mut h, setup(VENDOR_IN, 0x10, 5, 0, 3)),
        [0, 1, 2]
    );
    // a whole packet's worth that ends short of wLength: ZLP
    let t = transfer(&mut c, &mut h, setup(VENDOR_IN, 0x10, 64, 0, 100), &[]);
    assert_eq!(t.packets, [64, 0]);
    // nothing to say: one zero-length packet
    let t = transfer(&mut c, &mut h, setup(VENDOR_IN, 0x10, 0, 0, 100), &[]);
    assert_eq!(t.packets, [0]);
    // static bytes over several packets
    let t = transfer(&mut c, &mut h, setup(VENDOR_IN, 0x11, 0, 0, 0xffff), &[]);
    assert_eq!(t.data, BIG);
    assert_eq!(t.packets, [64, 64, 2]);
    // rejected by its handler
    assert!(stalls(&mut c, &mut h, setup(VENDOR_IN, 0x12, 0, 0, 1)));
}

#[test]
fn class_out_requests() {
    let (mut c, mut h) = configured::<Dev>();
    let data: Vec<u8> = (0..64).collect();
    // within the 64-byte buffer: one packet
    let t = transfer(&mut c, &mut h, setup(VENDOR_OUT, 0x30, 0, 0, 64), &data);
    assert!(t.status_in && !t.stalled);
    // without data stage
    let t = transfer(&mut c, &mut h, setup(VENDOR_OUT, 0x30, 0, 0, 0), &[]);
    assert!(t.status_in && !t.stalled);
    assert_eq!(h.out, [(0x30, data.clone()), (0x30, vec![])]);
    // rejected at the end of the data stage, and without one
    assert!(
        transfer(
            &mut c,
            &mut h,
            setup(VENDOR_OUT, 0x31, 0, 0, 4),
            &[1, 2, 3, 4]
        )
        .stalled
    );
    assert!(stalls(&mut c, &mut h, setup(VENDOR_OUT, 0x31, 0, 0, 0)));
    // longer than the buffer: refused at the SETUP
    let a = c.setup(setup(VENDOR_OUT, 0x30, 0, 0, 65), &mut h);
    assert_eq!(a.ep0, Ep0::Stall);
    // more data than wLength said
    let a = c.setup(setup(VENDOR_OUT, 0x30, 0, 0, 2), &mut h);
    assert_eq!(a.ep0, Ep0::Receive { last: true });
    assert_eq!(c.out_data(&[1, 2, 3], &mut h).ep0, Ep0::Stall);
}

#[test]
fn class_out_over_several_packets() {
    let mut c = Control::<Dev, 256>::new();
    let mut h = Rec::new();
    let data: Vec<u8> = (0..200).map(|i| i as u8).collect();
    let t = transfer(&mut c, &mut h, setup(VENDOR_OUT, 0x30, 0, 0, 200), &data);
    assert!(t.status_in && !t.stalled);
    assert_eq!(h.out, [(0x30, data)]);
    // a 200-byte answer from the bigger buffer
    let t = transfer(&mut c, &mut h, setup(VENDOR_IN, 0x10, 200, 0, 200), &[]);
    assert_eq!(t.packets, [64, 64, 64, 8]);
}

#[test]
fn handler_tuple_dispatch() {
    let mut c = Control::<Dev>::new();
    let mut h = (Rec::new(), Second::default());
    // the first takes 0x10, the second never sees it
    assert_eq!(
        ok_in(&mut c, &mut h, setup(VENDOR_IN, 0x10, 2, 0, 8)),
        [0, 1]
    );
    // 0x40 is the second's
    assert_eq!(
        ok_in(&mut c, &mut h, setup(VENDOR_IN, 0x40, 0, 0, 8)),
        [0x99]
    );
    assert!(!stalls(&mut c, &mut h, setup(VENDOR_OUT, 0x40, 0, 0, 0)));
    // a Reject from the first is final
    assert!(stalls(&mut c, &mut h, setup(VENDOR_IN, 0x12, 0, 0, 8)));
    // strings from whichever has them
    let want: [u8; string_len(SERIAL)] = string_descriptor(SERIAL);
    assert_eq!(
        ok_in(
            &mut c,
            &mut h,
            get_descriptor(descriptor_type::STRING, 3, 0xff)
        ),
        want
    );
    // events reach all
    c.reset(&mut h);
    assert_eq!(
        (h.0.events.as_slice(), h.1.resets),
        (&["reset".to_string()][..], 1)
    );
}

// --- bus reset ---

#[test]
fn bus_reset() {
    let (mut c, mut h) = configured::<Dev>();
    transfer(
        &mut c,
        &mut h,
        setup(0x01, request::SET_INTERFACE, 1, 1, 0),
        &[],
    );
    transfer(
        &mut c,
        &mut h,
        setup(0x02, request::SET_FEATURE, 0, 0x81, 0),
        &[],
    );
    transfer(
        &mut c,
        &mut h,
        setup(0x00, request::SET_FEATURE, 1, 0, 0),
        &[],
    );
    // in the middle of a transfer
    c.setup(
        get_descriptor(descriptor_type::CONFIGURATION, 0, 0xff),
        &mut h,
    );
    c.reset(&mut h);
    assert_eq!(c.state(), DeviceState::Default);
    assert_eq!(c.address(), 0);
    assert!(!c.remote_wakeup_enabled());
    assert_eq!(c.alt_setting(1), 0);
    assert!(!c.is_halted(ep(1, Direction::In)));
    assert!(!c.is_active(ep(1, Direction::In)));
    assert_eq!(c.in_complete().ep0, Ep0::Idle, "the transfer is gone");
    assert_eq!(h.events.last().unwrap(), "reset");
    // and enumerates again
    transfer(&mut c, &mut h, set_address(9), &[]);
    transfer(&mut c, &mut h, set_configuration(1), &[]);
    assert!(c.configured());
}

/// A whole enumeration the way Linux does it, as one sequence.
#[test]
fn linux_like_enumeration() {
    let mut c = Control::<Dev>::new();
    let mut h = Rec::new();
    c.reset(&mut h);
    assert_eq!(
        ok_in(
            &mut c,
            &mut h,
            get_descriptor(descriptor_type::DEVICE, 0, 64)
        ),
        Dev::DEVICE_DESC
    );
    c.reset(&mut h);
    let t = transfer(&mut c, &mut h, set_address(3), &[]);
    assert_eq!(t.effects, [Effect::SetAddress(3)]);
    ok_in(
        &mut c,
        &mut h,
        get_descriptor(descriptor_type::DEVICE, 0, 18),
    );
    let head = ok_in(
        &mut c,
        &mut h,
        get_descriptor(descriptor_type::CONFIGURATION, 0, 9),
    );
    let total = u16::from_le_bytes([head[2], head[3]]);
    assert_eq!(
        ok_in(
            &mut c,
            &mut h,
            get_descriptor(descriptor_type::CONFIGURATION, 0, total)
        ),
        Dev::CONFIG_DESC
    );
    ok_in(&mut c, &mut h, get_descriptor(descriptor_type::BOS, 0, 5));
    ok_in(
        &mut c,
        &mut h,
        get_descriptor(descriptor_type::BOS, 0, Dev::BOS.len() as u16),
    );
    ok_in(
        &mut c,
        &mut h,
        get_descriptor(descriptor_type::STRING, 0, 255),
    );
    for i in 1..=5 {
        ok_in(
            &mut c,
            &mut h,
            get_descriptor(descriptor_type::STRING, i, 255),
        );
    }
    let t = transfer(&mut c, &mut h, set_configuration(1), &[]);
    assert_eq!(t.effects, [Effect::Configure(true)]);
    assert!(c.is_active(ep(1, Direction::Out)) && c.is_active(ep(3, Direction::In)));
    assert_eq!(h.events, ["reset", "reset", "configured true"]);
}
