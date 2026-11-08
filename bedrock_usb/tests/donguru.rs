//! Golden test: Donguru's USB device (typec_dongle `fw/donguru`, `src/ww/mod.rs` and `src/cmsis_dap.rs`) as a
//! bedrock_usb `const` gives the same descriptors as embassy-usb 0.6's `Builder` built the way Donguru builds it.
//!
//! The embassy side runs on a dummy driver: its control pipe is fed GET_DESCRIPTOR and the MS OS 2.0 vendor
//! request, and what `UsbDevice` answers is compared byte for byte. The WireWeaver function repeats the builder
//! calls of `wire_weaver_usb_embassy::WireWeaverClass::with_endpoints` (wire_weaver 2d50b95e) with Donguru's values;
//! that crate itself needs defmt and an embassy-time driver, which a host test doesn't have. Donguru builds the
//! WireWeaver endpoints with the `endpoint_bulk_*_double_buffered` calls of its embassy fork; those differ from
//! `endpoint_bulk_*` only in the driver's allocation, not in the descriptor, so the plain calls stand in for them.
//!
//! Intentional differences, not compared here:
//! - Strings known only at run time (the serial number, the WireWeaver interface's API id with the user label)
//!   are `Str::Runtime`: bedrock_usb reserves the same indices (3 and 4) and the firmware serves the text.
//! - embassy-usb answers GET_DESCRIPTOR(DEVICE_QUALIFIER); bedrock_usb has no device qualifier, a full-speed-only
//!   device answers it with a request error (USB 2.0 §9.6.2), which is the control machine's part (USBD-2).
//! - The WireWeaver endpoints' double buffering is in the bedrock_usb descriptor (`Buffering::Double`) and so in
//!   its layout; embassy-usb keeps it in the driver.

use std::cell::RefCell;
use std::collections::VecDeque;
use std::future::Future;
use std::pin::pin;
use std::rc::Rc;
use std::task::{Context, Waker};

use bedrock_usb::Buffering::{Double, Single};
use bedrock_usb::desc::{
    Device, Endpoint, Function, Interface, Item, MsOs20, MsOsFeature, RegValue, STRING0, Str,
    WINDOWS_8_1, string_descriptor, string_len,
};
use bedrock_usb::layout::{RegMode, RegType};
use bedrock_usb::{Direction, Setup, descriptor_type, request};

use embassy_usb::driver::{
    self, Direction as EDirection, EndpointAddress as EAddress, EndpointAllocError, EndpointError,
    EndpointInfo, EndpointType, Event, Unsupported,
};
use embassy_usb::msos::{self, windows_version};
use embassy_usb::types::StringIndex;
use embassy_usb::{Builder, Config, Handler};

// Donguru's values (typec_dongle fw/donguru src/ww/mod.rs, src/cmsis_dap.rs; wire_weaver_usb_embassy 0.2.0)
const EP_WW_OUT: u8 = 1;
const EP_WW_IN: u8 = 2;
const EP_CMSIS_DAP: u8 = 3;
const MAX_PACKET: u16 = 64;
/// `ww_link::USB_INTERFACE_PROTOCOL`, the framing without `seq-prefix`
const WW_PROTOCOL: u8 = 0x37;
const WW_GUIDS: &[&str] = &["{4987DAA6-F852-4B79-A4C8-8C0E0648C845}"];
/// 0x40: "wwue" and wire_weaver_usb_embassy's own version
const WW_SELF_VERSION: [u8; 7] = [b'w', b'w', b'u', b'e', 0, 2, 0];
/// 0x41 / 0x42: the user API crate and its version, `donguru_api::DONGURU_API_FULL_GID`
const WW_USER_CRATE: &[u8] = b"donguru_api";
const WW_USER_VERSION: [u8; 3] = [0, 6, 0];
const DAP_STRING: &str = "CMSIS-DAP v2 Interface";
const API_ID: &str = "donguru_api 0.6.0 label";
const SERIAL: &str = "0123456789ABCDEF01234567";

const WW_FEATURES: &[MsOsFeature] = &[
    MsOsFeature::CompatibleId {
        id: "WINUSB",
        sub_id: "",
    },
    MsOsFeature::Registry {
        name: "DeviceInterfaceGUIDs",
        value: RegValue::MultiSz(WW_GUIDS),
    },
];

const DONGURU: Device = Device {
    manufacturer: Str::Const("donguru"),
    product: Str::Const("donguru"),
    serial: Str::Runtime,
    ms_os_20: Some(MsOs20 {
        vendor_code: 0,
        windows_version: WINDOWS_8_1,
        device: WW_FEATURES,
    }),
    functions: &[
        Function {
            class: 0xFF,
            subclass: 0,
            protocol: WW_PROTOCOL,
            interfaces: &[Interface {
                protocol: WW_PROTOCOL,
                string: Str::Runtime,
                items: &[
                    Item::Endpoint(Endpoint::bulk_out(EP_WW_OUT, MAX_PACKET, Double)),
                    Item::Endpoint(Endpoint::bulk_in(EP_WW_IN, MAX_PACKET, Double)),
                    Item::Class {
                        kind: 0x40,
                        data: &WW_SELF_VERSION,
                    },
                    Item::Class {
                        kind: 0x41,
                        data: WW_USER_CRATE,
                    },
                    Item::Class {
                        kind: 0x42,
                        data: &WW_USER_VERSION,
                    },
                ],
                ..Interface::vendor()
            }],
            ms_os: WW_FEATURES,
        },
        Function {
            interfaces: &[Interface {
                string: Str::Const(DAP_STRING),
                items: &[
                    Item::Endpoint(Endpoint::bulk_out(EP_CMSIS_DAP, MAX_PACKET, Single)),
                    Item::Endpoint(Endpoint::bulk_in(EP_CMSIS_DAP, MAX_PACKET, Single)),
                ],
                ..Interface::vendor()
            }],
            ..Function::vendor()
        },
    ],
    ..Device::new(0xc0de, 0xcafe)
};

bedrock_usb::descriptors!(Desc = DONGURU);

// --- embassy-usb on a dummy driver ---

/// What the control pipe was asked and how `UsbDevice` answered.
#[derive(Default)]
struct Script {
    setups: VecDeque<[u8; 8]>,
    /// One answer per setup: `Some(data stage)` or `None` for a STALL.
    answers: Vec<Option<Vec<u8>>>,
}

#[derive(Clone)]
struct DummyDriver(Rc<RefCell<Script>>);
struct DummyBus;
struct DummyControl(Rc<RefCell<Script>>);
struct DummyEp(EndpointInfo);

impl<'a> driver::Driver<'a> for DummyDriver {
    type EndpointOut = DummyEp;
    type EndpointIn = DummyEp;
    type ControlPipe = DummyControl;
    type Bus = DummyBus;

    fn alloc_endpoint_out(
        &mut self,
        ep_type: EndpointType,
        ep_addr: Option<EAddress>,
        max_packet_size: u16,
        interval_ms: u8,
    ) -> Result<DummyEp, EndpointAllocError> {
        Ok(DummyEp(EndpointInfo {
            addr: ep_addr.unwrap(),
            ep_type,
            max_packet_size,
            interval_ms,
        }))
    }

    fn alloc_endpoint_in(
        &mut self,
        ep_type: EndpointType,
        ep_addr: Option<EAddress>,
        max_packet_size: u16,
        interval_ms: u8,
    ) -> Result<DummyEp, EndpointAllocError> {
        Ok(DummyEp(EndpointInfo {
            addr: ep_addr.unwrap(),
            ep_type,
            max_packet_size,
            interval_ms,
        }))
    }

    fn start(self, _control_max_packet_size: u16) -> (DummyBus, DummyControl) {
        (DummyBus, DummyControl(self.0))
    }
}

impl driver::Bus for DummyBus {
    async fn enable(&mut self) {}
    async fn disable(&mut self) {}
    async fn poll(&mut self) -> Event {
        std::future::pending().await
    }
    fn endpoint_set_enabled(&mut self, _: EAddress, _: bool) {}
    fn endpoint_set_stalled(&mut self, _: EAddress, _: bool) {}
    fn endpoint_is_stalled(&mut self, _: EAddress) -> bool {
        false
    }
    async fn remote_wakeup(&mut self) -> Result<(), Unsupported> {
        Err(Unsupported)
    }
}

impl driver::Endpoint for DummyEp {
    fn info(&self) -> &EndpointInfo {
        &self.0
    }
    async fn wait_enabled(&mut self) {
        std::future::pending().await
    }
}

impl driver::EndpointOut for DummyEp {
    async fn read(&mut self, _: &mut [u8]) -> Result<usize, EndpointError> {
        std::future::pending().await
    }
}

impl driver::EndpointIn for DummyEp {
    async fn write(&mut self, _: &[u8]) -> Result<(), EndpointError> {
        std::future::pending().await
    }
}

impl driver::ControlPipe for DummyControl {
    fn max_packet_size(&self) -> usize {
        64
    }

    async fn setup(&mut self) -> [u8; 8] {
        let next = {
            let mut s = self.0.borrow_mut();
            let next = s.setups.pop_front();
            if next.is_some() {
                s.answers.push(Some(Vec::new()));
            }
            next
        };
        match next {
            Some(setup) => setup,
            None => std::future::pending().await,
        }
    }

    async fn data_out(&mut self, _: &mut [u8], _: bool, _: bool) -> Result<usize, EndpointError> {
        Ok(0)
    }

    async fn data_in(
        &mut self,
        data: &[u8],
        _first: bool,
        _last: bool,
    ) -> Result<(), EndpointError> {
        let mut s = self.0.borrow_mut();
        s.answers
            .last_mut()
            .unwrap()
            .as_mut()
            .unwrap()
            .extend_from_slice(data);
        Ok(())
    }

    async fn accept(&mut self) {}

    async fn reject(&mut self) {
        *self.0.borrow_mut().answers.last_mut().unwrap() = None;
    }

    async fn accept_set_address(&mut self, _: u8) {}
}

/// Serves the interface strings, as the WireWeaver and CMSIS-DAP `State` handlers do.
struct Strings {
    api_id: Option<StringIndex>,
    dap: Option<StringIndex>,
}

impl Handler for Strings {
    fn get_string(&mut self, index: StringIndex, _lang_id: u16) -> Option<&str> {
        if Some(index) == self.api_id {
            Some(API_ID)
        } else if Some(index) == self.dap {
            Some(DAP_STRING)
        } else {
            None
        }
    }
}

/// Donguru's `start()` and the two classes' constructors, on the dummy driver; answers `setups` in order.
fn embassy_answers(setups: &[Setup]) -> Vec<Option<Vec<u8>>> {
    let script = Rc::new(RefCell::new(Script {
        setups: setups.iter().map(|s| s.to_bytes()).collect(),
        ..Default::default()
    }));
    let leak = |n: usize| -> &'static mut [u8] { Box::leak(vec![0u8; n].into_boxed_slice()) };
    let strings: &'static mut Strings = Box::leak(Box::new(Strings {
        api_id: None,
        dap: None,
    }));

    let mut config = Config::new(0xc0de, 0xcafe);
    config.manufacturer = Some("donguru");
    config.product = Some("donguru");
    config.serial_number = Some(SERIAL);
    config.device_class = 0xEF;
    config.device_sub_class = 0x02;
    config.device_protocol = 0x01;
    config.composite_with_iads = true;
    config.max_power = 100;
    config.self_powered = false;

    let mut builder = Builder::new(
        DummyDriver(script.clone()),
        config,
        leak(192),
        leak(40),
        leak(330),
        leak(256),
    );

    // WireWeaverClass::with_endpoints
    builder.msos_descriptor(windows_version::WIN8_1, 0);
    builder.msos_feature(msos::CompatibleIdFeatureDescriptor::new("WINUSB", ""));
    builder.msos_feature(msos::RegistryPropertyFeatureDescriptor::new(
        "DeviceInterfaceGUIDs",
        msos::PropertyData::RegMultiSz(WW_GUIDS),
    ));
    {
        let mut func = builder.function(0xFF, 0x00, WW_PROTOCOL);
        func.msos_feature(msos::CompatibleIdFeatureDescriptor::new("WINUSB", ""));
        func.msos_feature(msos::RegistryPropertyFeatureDescriptor::new(
            "DeviceInterfaceGUIDs",
            msos::PropertyData::RegMultiSz(WW_GUIDS),
        ));
        let mut iface = func.interface();
        let api_id_index = iface.string();
        strings.api_id = Some(api_id_index);
        let mut alt = iface.alt_setting(0xFF, 0x00, WW_PROTOCOL, Some(api_id_index));
        alt.endpoint_bulk_out(
            Some(EAddress::from_parts(EP_WW_OUT as usize, EDirection::Out)),
            MAX_PACKET,
        );
        alt.endpoint_bulk_in(
            Some(EAddress::from_parts(EP_WW_IN as usize, EDirection::In)),
            MAX_PACKET,
        );
        alt.descriptor(0x40, &WW_SELF_VERSION);
        alt.descriptor(0x41, WW_USER_CRATE);
        alt.descriptor(0x42, &WW_USER_VERSION);
    }
    // CmsisDapClass::new
    {
        let mut func = builder.function(0xFF, 0x00, 0x00);
        let mut iface = func.interface();
        let name_index = iface.string();
        strings.dap = Some(name_index);
        let mut alt = iface.alt_setting(0xFF, 0x00, 0x00, Some(name_index));
        alt.endpoint_bulk_out(
            Some(EAddress::from_parts(EP_CMSIS_DAP as usize, EDirection::Out)),
            MAX_PACKET,
        );
        alt.endpoint_bulk_in(
            Some(EAddress::from_parts(EP_CMSIS_DAP as usize, EDirection::In)),
            MAX_PACKET,
        );
    }
    builder.handler(strings);

    let mut usb = builder.build();
    {
        // every step is ready at once: one poll answers all setups, then setup() waits forever
        let mut run = pin!(usb.run_until_suspend());
        let mut cx = Context::from_waker(Waker::noop());
        assert!(run.as_mut().poll(&mut cx).is_pending());
    }
    drop(usb);
    Rc::try_unwrap(script).ok().unwrap().into_inner().answers
}

const fn get_descriptor(kind: u8, index: u8, lang: u16) -> Setup {
    Setup {
        request_type: 0x80,
        request: request::GET_DESCRIPTOR,
        value: ((kind as u16) << 8) | index as u16,
        index: lang,
        length: 0xffff,
    }
}

#[test]
fn donguru_matches_embassy_usb() {
    let ms_os_request = Setup {
        request_type: 0xC0, // IN, vendor, device
        request: 0,         // the vendor code
        value: 0,
        index: request::MS_OS_20_DESCRIPTOR_INDEX,
        length: 0xffff,
    };
    let setups = [
        get_descriptor(descriptor_type::DEVICE, 0, 0),
        get_descriptor(descriptor_type::CONFIGURATION, 0, 0),
        get_descriptor(descriptor_type::BOS, 0, 0),
        ms_os_request,
        get_descriptor(descriptor_type::STRING, 0, 0),
        get_descriptor(descriptor_type::STRING, 1, 0x0409),
        get_descriptor(descriptor_type::STRING, 2, 0x0409),
        get_descriptor(descriptor_type::STRING, 3, 0x0409),
        get_descriptor(descriptor_type::STRING, 4, 0x0409),
        get_descriptor(descriptor_type::STRING, 5, 0x0409),
        get_descriptor(descriptor_type::STRING, 6, 0x0409),
    ];
    let answers = embassy_answers(&setups);
    assert_eq!(answers.len(), setups.len());
    let answer = |i: usize| answers[i].as_deref().expect("embassy-usb stalled");

    assert_eq!(answer(0), &Desc::DEVICE_DESC[..], "device descriptor");
    assert_eq!(
        answer(1),
        &Desc::CONFIG_DESC[..],
        "configuration descriptor"
    );
    assert_eq!(answer(2), &Desc::BOS[..], "BOS descriptor");
    assert_eq!(
        answer(3),
        &Desc::MS_OS_20_SET[..],
        "MS OS 2.0 descriptor set"
    );
    assert_eq!(answer(4), &STRING0[..], "string 0");

    // the constant strings, encoded at compile time
    const MANUFACTURER: [u8; string_len("donguru")] = string_descriptor("donguru");
    const DAP: [u8; string_len(DAP_STRING)] = string_descriptor(DAP_STRING);
    assert_eq!(DONGURU.string(1), Some(Str::Const("donguru")));
    assert_eq!(answer(5), &MANUFACTURER[..]);
    assert_eq!(answer(6), &MANUFACTURER[..]);
    assert_eq!(DONGURU.string(5), Some(Str::Const(DAP_STRING)));
    assert_eq!(answer(9), &DAP[..]);
    // the run-time ones on the same indices: serial 3, API id 4
    assert_eq!(DONGURU.string(3), Some(Str::Runtime));
    assert_eq!(
        answer(7)[2..],
        SERIAL
            .encode_utf16()
            .flat_map(u16::to_le_bytes)
            .collect::<Vec<_>>()[..]
    );
    assert_eq!(DONGURU.string(4), Some(Str::Runtime));
    assert_eq!(
        answer(8)[2..],
        API_ID
            .encode_utf16()
            .flat_map(u16::to_le_bytes)
            .collect::<Vec<_>>()[..]
    );
    // and nothing beyond
    assert_eq!(answers[10], None);
    assert_eq!(DONGURU.string(6), None);
}

#[test]
fn donguru_hand_checked() {
    // the lengths Donguru's UsbBuffers were sized for: 192 / 40 / 330
    assert_eq!(
        Desc::CONFIG_DESC.len(),
        9 + (8 + 9 + 7 + 7 + 9 + 13 + 5) + (8 + 9 + 7 + 7)
    );
    assert_eq!(Desc::BOS.len(), 5 + 7 + 28);
    assert_eq!(Desc::MS_OS_20_SET.len(), 10 + 2 * (20 + 132) + 8 + 8);
    #[rustfmt::skip]
    let head: &[u8] = &[
        9, 2, 98, 0, 2, 1, 0, 0x80, 50,
        8, 11, 0, 1, 0xff, 0, 0x37, 0,
        9, 4, 0, 0, 2, 0xff, 0, 0x37, 4,
        7, 5, 0x01, 0x02, 64, 0, 0,
        7, 5, 0x82, 0x02, 64, 0, 0,
        9, 0x40, b'w', b'w', b'u', b'e', 0, 2, 0,
    ];
    assert_eq!(&Desc::CONFIG_DESC[..head.len()], head);
    assert_eq!(
        &Desc::CONFIG_DESC[98 - 23..],
        &[
            9, 4, 1, 0, 2, 0xff, 0, 0, 5, 7, 5, 0x03, 0x02, 64, 0, 0, 7, 5, 0x83, 0x02, 64, 0, 0,
        ]
    );
}

#[test]
fn donguru_layout() {
    let l = Desc::LAYOUT;
    assert_eq!((l.count, l.table_len), (4, 32));
    assert_eq!(l.regs[0].reg_type, RegType::Control);
    assert_eq!(
        l.regs[1].mode,
        RegMode::Double {
            direction: Direction::Out,
            size: 64
        }
    );
    assert_eq!(
        l.regs[2].mode,
        RegMode::Double {
            direction: Direction::In,
            size: 64
        }
    );
    assert_eq!(
        l.regs[3].mode,
        RegMode::Single {
            out_size: 64,
            in_size: 64
        }
    );
    let rx64 = 0x8000_0000 | (1 << 26);
    assert_eq!(l.regs[0].bd, [32, 96 | rx64]);
    assert_eq!(l.regs[1].bd, [160 | rx64, 224 | rx64]);
    assert_eq!(l.regs[2].bd, [288, 352]);
    assert_eq!(l.regs[3].bd, [416, 480 | rx64]);
    assert_eq!(l.end, 544);
    const _: () = assert!(Desc::has(Direction::Out, 1, bedrock_usb::Buffering::Double));
    const _: () = assert!(Desc::has(Direction::In, 3, bedrock_usb::Buffering::Single));
    const _: () = assert!(!Desc::has(Direction::In, 1, bedrock_usb::Buffering::Double));
}

/// The same device with the WireWeaver function from `ww_function!` (feature `ww`): every descriptor array equals
/// the hand-written one above, which the golden test holds to embassy-usb's builder. Only the 0x40 payload, the
/// crate tag and version of the stack, is given as embassy's `wwue` 0.2.0 here; the macro's own default names
/// bedrock_usb (`ww::SELF_ID`).
#[cfg(feature = "ww")]
mod ww_macro {
    use super::*;
    use bedrock_usb::ww::MS_OS_20;

    const DONGURU_MACRO: Device = Device {
        manufacturer: Str::Const("donguru"),
        product: Str::Const("donguru"),
        serial: Str::Runtime,
        ms_os_20: Some(MS_OS_20),
        functions: &[
            bedrock_usb::ww_function! {
                out: EP_WW_OUT, in: EP_WW_IN, max_packet: MAX_PACKET, buffering: Double,
                user_crate: "donguru_api", user_version: (0, 6, 0), api_id: Str::Runtime,
                framing: bedrock_usb::ww::__ww_link::UsbFraming, self_id: WW_SELF_VERSION,
            },
            Function {
                interfaces: &[Interface {
                    string: Str::Const(DAP_STRING),
                    items: &[
                        Item::Endpoint(Endpoint::bulk_out(EP_CMSIS_DAP, MAX_PACKET, Single)),
                        Item::Endpoint(Endpoint::bulk_in(EP_CMSIS_DAP, MAX_PACKET, Single)),
                    ],
                    ..Interface::vendor()
                }],
                ..Function::vendor()
            },
        ],
        ..Device::new(0xc0de, 0xcafe)
    };
    bedrock_usb::descriptors!(DescMacro = DONGURU_MACRO);

    #[test]
    fn ww_function_equals_the_hand_written_one() {
        assert_eq!(DescMacro::DEVICE_DESC, Desc::DEVICE_DESC);
        assert_eq!(DescMacro::CONFIG_DESC, Desc::CONFIG_DESC);
        assert_eq!(DescMacro::BOS, Desc::BOS);
        assert_eq!(DescMacro::MS_OS_20_SET, Desc::MS_OS_20_SET);
    }
}
