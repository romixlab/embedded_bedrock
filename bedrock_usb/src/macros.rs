/// Names the descriptor arrays and the layout of a `const` [Device](crate::desc::Device), and checks it.
///
/// `descriptors!(pub Desc = DEVICE)` makes a unit struct `Desc` with
/// - `Desc::DEVICE`, the device itself;
/// - `Desc::DEVICE_DESC: [u8; 18]`, `Desc::CONFIG_DESC`, `Desc::BOS`, `Desc::MS_OS_20_SET`, each `[u8; N]` with
///   its exact length (the last two empty when the device has none), and [STRING0](crate::desc::STRING0) is the
///   same for every device;
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
        $vis struct $name;

        #[allow(dead_code)]
        impl $name {
            pub const DEVICE: $crate::desc::Device = $device;
            pub const DEVICE_DESC: [u8; 18] = $device.device_descriptor();
            pub const CONFIG_DESC: [u8; $device.config_len()] = $device.config_descriptor();
            pub const BOS: [u8; $device.bos_len()] = $device.bos_descriptor();
            pub const MS_OS_20_SET: [u8; $device.ms_os_20_len()] = $device.ms_os_20_set();
            pub const LAYOUT: $crate::layout::Layout = $crate::layout::Layout::new(&$device);

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
            assert!($name::LAYOUT.count >= 1);
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
