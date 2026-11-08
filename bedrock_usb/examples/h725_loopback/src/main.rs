//! bedrock_usb on the STM32H725: OTG_HS over ULPI with an external high-speed PHY (a USB3315 on B135), a vendor
//! interface with a double-buffered bulk OUT (EP1) and IN (EP2) that sends back every packet it gets: 512-byte
//! packets at high speed, 64 at full speed. Windows binds WinUSB to it without a driver install (MS OS 2.0
//! descriptors); the serial number is the chip's UID.
//!
//! Clocks: embassy-stm32's defaults, HSI 64 MHz for the core and the AHB (OTG_HS needs 30 MHz or more); the PHY
//! clocks the core over ULPI with its own 60 MHz. Supply: embassy-stm32's default (LDO), VDD33USB from outside.
//! A board's firmware sets both from its own design.
//!
//! ULPI pins: CK PA5, DIR PC2, NXT PC3, STP PC0, D0 PA3, D1 PB0, D2 PB1, D3 PB10, D4 PB11, D5 PB12, D6 PB13,
//! D7 PB5.
//!
//! Run on a board: `cargo run --release` in this directory (probe-rs, flip-link). Built for size by `just lint`
//! at opt-level z and 3.

#![no_std]
#![no_main]

use bedrock_usb::buffer::Double;
use bedrock_usb::control::Handler;
use bedrock_usb::desc::{
    Device, Endpoint, Function, Interface, Item, MsOs20, MsOsFeature, RegValue, STRING_SERIAL, Str,
    WINDOWS_8_1,
};
use bedrock_usb::otg::embassy::{InterruptHandler, Supply, Ulpi};
use bedrock_usb::otg::{BulkIn, BulkOut, Core, Driver, Mmio};
use bedrock_usb::v4::Status;
use bedrock_usb::{Buffering, Usb};
use defmt::{info, warn};
use embassy_executor::Spawner;
use embassy_stm32::bind_interrupts;
use static_cell::StaticCell;
use {defmt_rtt as _, panic_probe as _};

/// HCLK: HSI at 64 MHz, AHB undivided.
const HCLK_HZ: u32 = 64_000_000;

const OUT: u8 = 1;
const IN: u8 = 2;

const WINUSB: &[MsOsFeature] = &[
    MsOsFeature::CompatibleId {
        id: "WINUSB",
        sub_id: "",
    },
    // the interface GUID host software opens the device by; this one is the example's own
    MsOsFeature::Registry {
        name: "DeviceInterfaceGUIDs",
        value: RegValue::MultiSz(&["{F612EFD3-8757-471D-AF38-1DD9B05A6A62}"]),
    },
];

const DEVICE: Device = Device {
    manufacturer: Str::Const("embedded_bedrock"),
    product: Str::Const("bedrock_usb HS loopback"),
    serial: Str::Runtime,
    high_speed: true,
    ms_os_20: Some(MsOs20 {
        vendor_code: 0x01,
        windows_version: WINDOWS_8_1,
        device: WINUSB,
    }),
    functions: &[Function {
        interfaces: &[Interface {
            string: Str::Const("loopback"),
            items: &[
                // 64 at full speed, 512 at high speed
                Item::Endpoint(Endpoint::bulk_out(OUT, 64, Buffering::Double)),
                Item::Endpoint(Endpoint::bulk_in(IN, 64, Buffering::Double)),
            ],
            ..Interface::vendor()
        }],
        ..Function::vendor()
    }],
    // the test VID / PID of embassy's examples
    ..Device::new(0xc0de, 0xcafe)
};
bedrock_usb::descriptors!(Desc = DEVICE, otg = Core::H7_OTG_HS);
bedrock_usb::otg_state!(static USB_STATE: Desc);

bind_interrupts!(struct Irqs {
    OTG_HS => InterruptHandler;
});

/// The firmware's part of EP0: the serial number string.
struct Serial {
    hex: [u8; 24],
}

impl Serial {
    fn from_uid(uid: &[u8; 12]) -> Self {
        const DIGITS: &[u8; 16] = b"0123456789ABCDEF";
        let mut hex = [0u8; 24];
        for (i, b) in uid.iter().enumerate() {
            hex[2 * i] = DIGITS[(b >> 4) as usize];
            hex[2 * i + 1] = DIGITS[(b & 0xf) as usize];
        }
        Serial { hex }
    }
}

impl Handler for Serial {
    fn string(&self, index: u8) -> Option<&str> {
        (index == STRING_SERIAL).then(|| core::str::from_utf8(&self.hex).unwrap_or("0"))
    }
}

type Device_ = Usb<Desc, Driver<Desc, Mmio>, Serial>;

#[embassy_executor::task]
async fn usb_task(usb: &'static mut Device_) -> ! {
    usb.run().await
}

/// Watches the bus status, as a firmware would to mark a boot healthy or notice a hub bouncing its port.
#[embassy_executor::task]
async fn status_task() -> ! {
    let mut seen = Status::new();
    loop {
        let s = USB_STATE.status_changed(seen).await;
        if s.resets_after_configured != seen.resets_after_configured {
            warn!("bus reset while configured ({})", s.resets_after_configured);
        }
        info!("usb {}", s);
        seen = s;
    }
}

#[embassy_executor::task]
async fn loopback(mut out: BulkOut<Mmio, OUT, Double>, mut inp: BulkIn<Mmio, IN, Double>) -> ! {
    let mut buf = [0u8; 512];
    loop {
        out.wait_enabled().await;
        // a disabled endpoint (bus reset, SET_CONFIGURATION 0) ends the round: wait for the next configuration
        while let Ok(n) = out.read(&mut buf).await {
            if inp.write(&buf[..n]).await.is_err() {
                break;
            }
        }
    }
}

#[embassy_executor::main]
async fn main(spawner: Spawner) {
    let p = embassy_stm32::init(embassy_stm32::Config::default());
    info!("h725_loopback");

    let ulpi = Ulpi {
        clk: p.PA5,
        dir: p.PC2,
        nxt: p.PC3,
        stp: p.PC0,
        d0: p.PA3,
        d1: p.PB0,
        d2: p.PB1,
        d3: p.PB10,
        d4: p.PB11,
        d5: p.PB12,
        d6: p.PB13,
        d7: p.PB5,
    };
    let driver =
        bedrock_usb::otg::embassy::new_ulpi::<Desc, _, _, _, _, _, _, _, _, _, _, _, _, _, _>(
            p.USB_OTG_HS,
            Irqs,
            ulpi,
            true,
            Supply::External,
            &USB_STATE,
            HCLK_HZ,
        );
    let out = driver.bulk_out::<OUT, Double>();
    let inp = driver.bulk_in::<IN, Double>();
    driver.connect();

    static USB: StaticCell<Device_> = StaticCell::new();
    let usb = USB.init(Usb::new(
        driver,
        Serial::from_uid(&embassy_stm32::uid::uid()),
    ));
    spawner.spawn(usb_task(usb).unwrap());
    spawner.spawn(status_task().unwrap());
    spawner.spawn(loopback(out, inp).unwrap());
}
