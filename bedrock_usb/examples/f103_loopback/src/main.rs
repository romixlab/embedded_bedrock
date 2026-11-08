//! bedrock_usb on usb_v1, the STM32F103 of a "blue pill" board: the G0B1 example's device on the peripheral with
//! 512 bytes of 16-bit USB SRAM, each half-word in a 32-bit word. A vendor interface with a double-buffered bulk
//! OUT (EP1) and IN (EP2) that sends back every packet it gets; the two take 408 of the 512 bytes. Windows binds WinUSB to it without a driver install (MS OS 2.0 descriptors); the
//! serial number is the chip's UID.
//!
//! Clocks: SYSCLK 72 MHz from the board's 8 MHz crystal through the PLL; USB 48 MHz from the PLL through the USB
//! prescaler (1.5, which embassy-stm32 picks); APB1 36 MHz, its maximum, above the 13 MHz ES096 §2.12.4 asks for
//! (bedrock_usb::v1, errata).
//!
//! The DP pull-up: the board's fixed resistor on PA12 (`PullUp::External`). So that the host sees a new device
//! after a reset or a flash, PA12 is driven low for 10 ms before the driver starts, as embassy's F1 example does.
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
use bedrock_usb::v1::embassy::InterruptHandler;
use bedrock_usb::v1::{BulkIn, BulkOut, Driver, Mmio, PullUp, Spaced512, Status};
use bedrock_usb::{Buffering, Usb};
use defmt::{info, warn};
use embassy_executor::Spawner;
use embassy_stm32::gpio::{Level, Output, Speed};
use embassy_stm32::rcc::{
    AHBPrescaler, APBPrescaler, Hse, HseMode, Pll, PllMul, PllPreDiv, PllSource, Sysclk,
};
use embassy_stm32::time::Hertz;
use embassy_stm32::{bind_interrupts, peripherals};
use static_cell::StaticCell;
use {defmt_rtt as _, panic_probe as _};

/// HCLK, which the driver's short waits (tSTARTUP, the OUT-packet wait) count in.
const HCLK_HZ: u32 = 72_000_000;

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
    USB_LP_CAN1_RX0 => InterruptHandler<peripherals::USB, Spaced512>;
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

type Device_ = Usb<Desc, Driver<Desc, Mmio<Spaced512>>, Serial>;

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
async fn loopback(
    mut out: BulkOut<Mmio<Spaced512>, OUT, Double>,
    mut inp: BulkIn<Mmio<Spaced512>, IN, Double>,
) -> ! {
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
    // SYSCLK 72 MHz: HSE 8 MHz * 9; AHB undivided, APB1 / 2 (36 MHz at most), APB2 undivided
    config.rcc.hse = Some(Hse {
        freq: Hertz(8_000_000),
        mode: HseMode::Oscillator,
    });
    config.rcc.pll = Some(Pll {
        src: PllSource::HSE,
        prediv: PllPreDiv::DIV1,
        mul: PllMul::MUL9,
    });
    config.rcc.sys = Sysclk::PLL1_P;
    config.rcc.ahb_pre = AHBPrescaler::DIV1;
    config.rcc.apb1_pre = APBPrescaler::DIV2;
    config.rcc.apb2_pre = APBPrescaler::DIV1;
    let mut p = embassy_stm32::init(config);
    info!("f103_loopback");

    // DP low for 10 ms: the host sees a disconnect, then the board's pull-up again once the driver has the pin
    {
        let _dp = Output::new(p.PA12.reborrow(), Level::Low, Speed::Low);
        cortex_m::asm::delay(HCLK_HZ / 100);
    }

    let driver = bedrock_usb::v1::embassy::new::<Desc, _, Spaced512, _>(
        p.USB,
        Irqs,
        p.PA12,
        p.PA11,
        &USB_STATE,
        PullUp::External,
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
