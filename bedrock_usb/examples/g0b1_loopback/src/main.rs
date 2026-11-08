//! bedrock_usb on the STM32G0B1: a vendor interface with a double-buffered bulk OUT (EP1) and IN (EP2) that sends
//! back every packet it gets. Windows binds WinUSB to it without a driver install (MS OS 2.0 descriptors); the
//! serial number is the chip's UID.
//!
//! Clocks: SYSCLK 64 MHz from HSI16 through the PLL; USB from HSI48, trimmed to the host's SOF by the CRS.
//!
//! With feature `sof`, the interrupt handler timestamps every SOF with embassy-time and a task fits the local
//! clock (HSI16 through the PLL) against the host's frames with bedrock_sync's `SofTrack`, logging its rate.
//!
//! Run on a board: `cargo run --release` in this directory (probe-rs, flip-link), `--features sof` for the SOF
//! track. Built for size by `just lint` at opt-level z and 3, and with `sof` at z.

#![no_std]
#![no_main]

use bedrock_usb::buffer::Double;
use bedrock_usb::control::Handler;
use bedrock_usb::desc::{
    Device, Endpoint, Function, Interface, Item, MsOs20, MsOsFeature, RegValue, STRING_SERIAL, Str,
    WINDOWS_8_1,
};
use bedrock_usb::v4::embassy::InterruptHandler;
use bedrock_usb::v4::{BulkIn, BulkOut, Mmio, Status};
use bedrock_usb::{Buffering, Usb};
use defmt::{info, warn};
use embassy_executor::Spawner;
use embassy_stm32::rcc::{
    AHBPrescaler, APBPrescaler, Hsi, Hsi48Config, HsiSysDiv, Pll, PllMul, PllPreDiv, PllRDiv,
    PllSource, Sysclk, mux,
};
use embassy_stm32::{bind_interrupts, peripherals};
use static_cell::StaticCell;
use {defmt_rtt as _, panic_probe as _};

/// HCLK, which the driver's short waits (tSTARTUP, ES0548 2.16.1) count in.
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
#[cfg(not(feature = "sof"))]
bedrock_usb::state!(static USB_STATE: Desc);
#[cfg(feature = "sof")]
bedrock_usb::state!(static USB_STATE: Desc, sof = now_us);

/// The local clock the SOFs are stamped with, from the interrupt handler.
#[cfg(feature = "sof")]
fn now_us() -> u64 {
    embassy_time::Instant::now().as_micros()
}

/// Hands the last SOF to the track ten times a second and logs the local clock against the frames.
#[cfg(feature = "sof")]
#[embassy_executor::task]
async fn sof_task() -> ! {
    use bedrock_sync::sof::{Push, SofTrack};
    let mut track = SofTrack::new();
    let mut ticks = 0u32;
    loop {
        embassy_time::Timer::after_millis(100).await;
        if let Some(s) = USB_STATE.sof() {
            if let Push::Restarted(why) = track.push(s.frame, s.local_us) {
                info!("sof track restarted: {}", why);
            }
            ticks += 1;
            if ticks.is_multiple_of(100) {
                info!(
                    "sof {} frame {} rate {} ppb, {} missed",
                    track.state(),
                    s.frame,
                    track.rate_ppb(),
                    s.missed
                );
            }
        }
    }
}

bind_interrupts!(struct Irqs {
    USB_UCPD1_2 => InterruptHandler<peripherals::USB>;
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

type Device_ = Usb<Desc, Mmio, Serial>;

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
    // SYSCLK 64 MHz: HSI16 / 1 * 8 = 128 MHz VCO, / 2; AHB and APB undivided (R10: APB at least 12 MHz)
    config.rcc.hsi = Some(Hsi {
        sys_div: HsiSysDiv::DIV1,
    });
    config.rcc.pll = Some(Pll {
        source: PllSource::HSI,
        prediv: PllPreDiv::DIV1,
        mul: PllMul::MUL8,
        divp: None,
        divq: None,
        divr: Some(PllRDiv::DIV2),
    });
    config.rcc.sys = Sysclk::PLL1_R;
    config.rcc.ahb_pre = AHBPrescaler::DIV1;
    config.rcc.apb1_pre = APBPrescaler::DIV1;
    // USB from HSI48, trimmed by the CRS from the host's SOF
    config.rcc.hsi48 = Some(Hsi48Config {
        sync_from_usb: true,
    });
    config.rcc.mux.usbsel = mux::Usbsel::HSI48;
    let p = embassy_stm32::init(config);
    info!("g0b1_loopback");

    let driver = bedrock_usb::v4::embassy::new::<Desc, _, _>(
        p.USB, Irqs, p.PA12, p.PA11, &USB_STATE, HCLK_HZ,
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
    #[cfg(feature = "sof")]
    spawner.spawn(sof_task().unwrap());
}
