/// Names the descriptor arrays and the layout of a `const` [Device](crate::desc::Device), and checks it.
///
/// `descriptors!(pub Desc = DEVICE)` makes a unit struct `Desc` with
/// - `Desc::DEVICE`, the device itself;
/// - `Desc::DEVICE_DESC: [u8; 18]`, `Desc::CONFIG_DESC`, `Desc::CONFIG_DESC_HS`, `Desc::DEVICE_QUALIFIER`,
///   `Desc::BOS`, `Desc::MS_OS_20_SET`, each `[u8; N]` with its exact length (empty when the device has none: the
///   high-speed two on a full-speed-only device), and [STRING0](crate::desc::STRING0) is the same for every device;
/// - `Desc::LAYOUT`, the endpoint registers and USB SRAM ([Layout](crate::layout::Layout));
/// - `const fn Desc::has(direction, number, buffering)`, for a typed endpoint's `const { assert!(..) }`;
/// - and implements [Descriptors](crate::Descriptors) for code that takes the device as a type.
///
/// Everything is evaluated at compile time, so a device the descriptors or the peripheral can't serve fails the
/// build with the reason, e.g. a double-buffered endpoint that shares its register:
///
/// ```compile_fail
/// use bedrock_usb::desc::{Device, Endpoint, Function, Interface, Item};
/// use bedrock_usb::Buffering::{Double, Single};
///
/// const DEVICE: Device = Device {
///     functions: &[Function {
///         interfaces: &[Interface {
///             items: &[
///                 Item::Endpoint(Endpoint::bulk_out(1, 64, Double)),
///                 Item::Endpoint(Endpoint::bulk_in(1, 64, Single)), // register 1 is taken by the OUT endpoint
///             ],
///             ..Interface::vendor()
///         }],
///         ..Function::vendor()
///     }],
///     ..Device::new(0xc0de, 0xcafe)
/// };
/// bedrock_usb::descriptors!(Desc = DEVICE);
/// ```
///
/// and asking for an endpoint the device lacks:
///
/// ```compile_fail
/// # use bedrock_usb::desc::{Device, Endpoint, Function, Interface, Item};
/// # use bedrock_usb::{Buffering, Direction};
/// # const DEVICE: Device = Device {
/// #     functions: &[Function {
/// #         interfaces: &[Interface { items: &[Item::Endpoint(Endpoint::bulk_in(1, 64, Buffering::Single))], ..Interface::vendor() }],
/// #         ..Function::vendor()
/// #     }],
/// #     ..Device::new(0xc0de, 0xcafe)
/// # };
/// bedrock_usb::descriptors!(Desc = DEVICE);
/// const _: () = assert!(Desc::has(Direction::In, 1, Buffering::Double), "EP1 IN is single-buffered");
/// ```
///
/// The same device with the right question builds:
///
/// ```
/// # use bedrock_usb::desc::{Device, Endpoint, Function, Interface, Item};
/// # use bedrock_usb::{Buffering, Direction};
/// # const DEVICE: Device = Device {
/// #     functions: &[Function {
/// #         interfaces: &[Interface { items: &[Item::Endpoint(Endpoint::bulk_in(1, 64, Buffering::Single))], ..Interface::vendor() }],
/// #         ..Function::vendor()
/// #     }],
/// #     ..Device::new(0xc0de, 0xcafe)
/// # };
/// bedrock_usb::descriptors!(Desc = DEVICE);
/// const _: () = assert!(Desc::has(Direction::In, 1, Buffering::Single));
/// assert_eq!(Desc::CONFIG_DESC.len(), 9 + 8 + 9 + 7);
/// assert_eq!(Desc::LAYOUT.count, 2);
/// ```
/// Byte `i` of `s`, `None` past its end: what [descriptors!]'s checks index empty arrays with (`slice::get` is not
/// const, and an index into an empty array is a lint error even behind `is_empty()`).
#[doc(hidden)]
pub const fn __byte(s: &[u8], i: usize) -> Option<u8> {
    if i < s.len() { Some(s[i]) } else { None }
}

#[macro_export]
macro_rules! descriptors {
    ($vis:vis $name:ident = $device:path) => {
        $crate::descriptors!(@common $vis $name = $device, $crate::layout::Layout::new(&$device));

        // evaluated whether or not the firmware uses them yet, so a bad device fails the build right here
        const _: () = assert!($name::LAYOUT.count >= 1);
    };
    // on a Synopsys OTG core: the FIFO layout on `$core` is checked instead of the usb_v4 layout, which the device
    // need not fit (endpoint 8, a double-buffered endpoint beside another on its number); a trait impl's consts
    // are evaluated whether used or not, so LAYOUT is the empty one
    ($vis:vis $name:ident = $device:path, otg = $core:expr) => {
        $crate::descriptors!(@common $vis $name = $device, $crate::layout::Layout::NONE);

        #[allow(dead_code)]
        impl $name {
            pub const OTG: $crate::otg::Layout = $crate::otg::Layout::new(&$device, &$core);
        }

        impl $crate::otg::OtgDescriptors for $name {
            const OTG: $crate::otg::Layout = $name::OTG;
        }

        const _: () = assert!($name::OTG.count >= 1);
    };
    (@common $vis:vis $name:ident = $device:path, $layout:expr) => {
        $vis struct $name;

        #[allow(dead_code)]
        impl $name {
            pub const DEVICE: $crate::desc::Device = $device;
            pub const DEVICE_DESC: [u8; 18] = $device.device_descriptor();
            pub const CONFIG_DESC: [u8; $device.config_len()] = $device.config_descriptor();
            pub const CONFIG_DESC_HS: [u8; $device.config_len_hs()] = $device.config_descriptor_hs();
            pub const DEVICE_QUALIFIER: [u8; $device.device_qualifier_len()] = $device.device_qualifier();
            pub const BOS: [u8; $device.bos_len()] = $device.bos_descriptor();
            pub const MS_OS_20_SET: [u8; $device.ms_os_20_len()] = $device.ms_os_20_set();
            /// The usb_v4 layout; [Layout::NONE](crate::layout::Layout::NONE) for a device declared for the OTG
            /// driver, which the usb_v4 driver refuses.
            pub const LAYOUT: $crate::layout::Layout = $layout;

            /// Whether the device has endpoint `number` in `direction` with `buffering`.
            pub const fn has(
                direction: $crate::Direction,
                number: u8,
                buffering: $crate::Buffering,
            ) -> bool {
                $device.has(direction, number, buffering)
            }
        }

        impl $crate::Descriptors for $name {
            const DEVICE: $crate::desc::Device = $device;
            const DEVICE_DESC: [u8; 18] = $name::DEVICE_DESC;
            const CONFIG_DESC: &'static [u8] = &$name::CONFIG_DESC;
            const CONFIG_DESC_HS: &'static [u8] = &$name::CONFIG_DESC_HS;
            const DEVICE_QUALIFIER: &'static [u8] = &$name::DEVICE_QUALIFIER;
            const BOS: &'static [u8] = &$name::BOS;
            const MS_OS_20_SET: &'static [u8] = &$name::MS_OS_20_SET;
            const LAYOUT: $crate::layout::Layout = $name::LAYOUT;
        }

        // evaluated whether or not the firmware uses them yet, so a bad device fails the build right here
        const _: () = {
            assert!($name::DEVICE_DESC[0] == 18);
            assert!($name::CONFIG_DESC[1] == $crate::descriptor_type::CONFIGURATION);
            assert!(matches!($crate::__byte(&$name::BOS, 1), None | Some($crate::descriptor_type::BOS)));
            assert!(matches!($crate::__byte(&$name::MS_OS_20_SET, 0), None | Some(10)));
        };
    };
}

/// Declares the `static` [State](crate::v4::State) of a device, sized by its layout: one entry per endpoint
/// register in use, nothing for the registers it leaves empty.
///
/// ```
/// # use bedrock_usb::desc::{Device, Endpoint, Function, Interface, Item};
/// # use bedrock_usb::Buffering;
/// # const DEVICE: Device = Device {
/// #     functions: &[Function {
/// #         interfaces: &[Interface { items: &[Item::Endpoint(Endpoint::bulk_in(2, 64, Buffering::Single))], ..Interface::vendor() }],
/// #         ..Function::vendor()
/// #     }],
/// #     ..Device::new(0xc0de, 0xcafe)
/// # };
/// bedrock_usb::descriptors!(Desc = DEVICE);
/// bedrock_usb::state!(static USB_STATE: Desc);
/// ```
///
/// With `sof = <fn() -> u64>`, the interrupt handler timestamps every SOF with that local clock in µs
/// ([State::with_sof](crate::v4::State::with_sof)); the firmware reads the last one with `USB_STATE.sof()`:
///
/// ```
/// # use bedrock_usb::desc::{Device, Endpoint, Function, Interface, Item};
/// # use bedrock_usb::Buffering;
/// # const DEVICE: Device = Device {
/// #     functions: &[Function {
/// #         interfaces: &[Interface { items: &[Item::Endpoint(Endpoint::bulk_in(2, 64, Buffering::Single))], ..Interface::vendor() }],
/// #         ..Function::vendor()
/// #     }],
/// #     ..Device::new(0xc0de, 0xcafe)
/// # };
/// # bedrock_usb::descriptors!(Desc = DEVICE);
/// fn now_us() -> u64 {
///     // embassy_time::Instant::now().as_micros() in a firmware
///     0
/// }
/// bedrock_usb::state!(static USB_STATE: Desc, sof = now_us);
/// assert!(USB_STATE.sof().is_none());
/// ```
#[macro_export]
macro_rules! state {
    ($vis:vis static $name:ident : $desc:ty) => {
        $vis static $name: $crate::v4::State<{ <$desc as $crate::Descriptors>::LAYOUT.count as usize }> =
            $crate::v4::State::new();
    };
    ($vis:vis static $name:ident : $desc:ty, sof = $clock:expr) => {
        $vis static $name: $crate::v4::State<{ <$desc as $crate::Descriptors>::LAYOUT.count as usize }> =
            $crate::v4::State::with_sof($clock);
    };
}

/// Declares the `static` [State](crate::otg::State) of a device on the OTG driver, sized by its OTG layout: one
/// entry per endpoint number in use, and the RAM its OUT packet buffers take.
///
/// ```
/// # use bedrock_usb::desc::{Device, Endpoint, Function, Interface, Item};
/// # use bedrock_usb::Buffering;
/// # const DEVICE: Device = Device {
/// #     high_speed: true,
/// #     functions: &[Function {
/// #         interfaces: &[Interface { items: &[Item::Endpoint(Endpoint::bulk_out(8, 64, Buffering::Double))], ..Interface::vendor() }],
/// #         ..Function::vendor()
/// #     }],
/// #     ..Device::new(0xc0de, 0xcafe)
/// # };
/// bedrock_usb::descriptors!(Desc = DEVICE, otg = bedrock_usb::otg::Core::H7_OTG_HS);
/// bedrock_usb::otg_state!(static USB_STATE: Desc);
/// assert_eq!(Desc::OTG.count, 9);
/// assert_eq!(Desc::OTG.out_words, 16 + 2 * 128);
/// ```
#[macro_export]
macro_rules! otg_state {
    ($vis:vis static $name:ident : $desc:ty) => {
        $vis static $name: $crate::otg::State<
            { <$desc as $crate::otg::OtgDescriptors>::OTG.count as usize },
            { <$desc as $crate::otg::OtgDescriptors>::OTG.out_words as usize },
        > = $crate::otg::State::new();
    };
}
