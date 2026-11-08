//! bedrock_usb on usb_v3, the STM32G474: the G0B1 example's device on the 16-bit USB SRAM peripheral. A vendor interface with a double-buffered bulk OUT (EP1) and IN (EP2) that sends
//! back every packet it gets. Windows binds WinUSB to it without a driver install (MS OS 2.0 descriptors); the
//! serial number is the chip's UID.
//!
//! Clocks: SYSCLK 170 MHz from HSI16 through the PLL (range 1 boost); USB from HSI48, trimmed to the host's SOF by
//! the CRS; APB1 undivided, well above the 10 MHz the USB packet memory needs (bedrock_usb::v3, errata).
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
use bedrock_usb::v3::embassy::InterruptHandler;
use bedrock_usb::v3::{BulkIn, BulkOut, Driver, Family, Mmio, Status};
use bedrock_usb::{Buffering, Usb};
use defmt::{info, warn};
use embassy_executor::Spawner;
use embassy_stm32::rcc::{
    AHBPrescaler, APBPrescaler, Hsi48Config, Pll, PllMul, PllPreDiv, PllRDiv, PllSource, Sysclk,
    mux,
};
use embassy_stm32::{bind_interrupts, peripherals};
use static_cell::StaticCell;
use {defmt_rtt as _, panic_probe as _};

/// HCLK, which the driver's short waits (tSTARTUP, the OUT-packet wait) count in.
const HCLK_HZ: u32 = 170_000_000;

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
    product: Str::Const("bedrock_usb loopback"),
    serial: Str::Runtime,
    ms_os_20: Some(MsOs20 {
        vendor_code: 0x01,
        windows_version: WINDOWS_8_1,
        device: WINUSB,
    }),
    functions: &[Function {
        interfaces: &[Interface {
            string: Str::Const("loopback"),
            items: &[
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
bedrock_usb::descriptors!(Desc = DEVICE);
bedrock_usb::state!(static USB_STATE: Desc);

bind_interrupts!(struct Irqs {
    USB_LP => InterruptHandler<peripherals::USB>;
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
    let mut buf = [0u8; 64];
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
    let mut config = embassy_stm32::Config::default();
    // SYSCLK 170 MHz: HSI16 / 4 * 85 = 340 MHz VCO, / 2; range 1 boost above 150 MHz; AHB and APB undivided
    config.rcc.hsi = true;
    config.rcc.pll = Some(Pll {
        source: PllSource::HSI,
        prediv: PllPreDiv::DIV4,
        mul: PllMul::MUL85,
        divp: None,
        divq: None,
        divr: Some(PllRDiv::DIV2),
    });
    config.rcc.sys = Sysclk::PLL1_R;
    config.rcc.boost = true;
    config.rcc.ahb_pre = AHBPrescaler::DIV1;
    config.rcc.apb1_pre = APBPrescaler::DIV1;
    config.rcc.apb2_pre = APBPrescaler::DIV1;
    // USB from HSI48, trimmed by the CRS from the host's SOF
    config.rcc.hsi48 = Some(Hsi48Config {
        sync_from_usb: true,
    });
    config.rcc.mux.clk48sel = mux::Clk48sel::HSI48;
    let p = embassy_stm32::init(config);
    info!("g474_loopback");

    let driver = bedrock_usb::v3::embassy::new::<Desc, _, _>(
        p.USB,
        Irqs,
        p.PA12,
        p.PA11,
        &USB_STATE,
        Family::G4,
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
