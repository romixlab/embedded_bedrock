//! `CLK-5` USB enabled without an exact 48 MHz source

use crate::check::{Check, CheckMeta, Need, Registered};
use crate::finding::Finding;
use crate::model::Firmware;
use crate::model::scan;

pub struct Clk5;

static META: CheckMeta = CheckMeta {
    id: "CLK-5",
    needs: &[Need::Sources],
};

inventory::submit!(Registered(&Clk5));

/// Words of a USB device stack in the firmware
const USB: &[&str] = &[
    "embassy_usb",
    "usb_device",
    "UsbBus",
    "UsbDeviceBuilder",
    "synopsys_usb_otg",
];

impl Check for Clk5 {
    fn meta(&self) -> &'static CheckMeta {
        &META
    }

    /// An STM32 firmware with a USB device (embassy-usb, usb-device, an `usb::Driver`) whose source configures no
    /// 48 MHz source: no HSI48 (with CRS), no `clk48` / MSI PLL mode, and not the HSE together with a PLL. USB
    /// needs 48 MHz within 0.25 %; the internal RC oscillators are not that close untrimmed, and on parts without
    /// HSI48 (F1, F4, L476, ...) only a crystal through a PLL is.
    fn run(&self, fw: &Firmware) -> Vec<Finding> {
        if !scan::is_stm32(fw) {
            return Vec::new();
        }
        let words = scan::words(fw);
        let usb = words
            .iter()
            .find(|w| USB.contains(&w.text.as_str()))
            .or_else(|| {
                // `usb::Driver::new..(`: the word `Driver` right after a `usb` path segment
                words
                    .windows(2)
                    .find(|p| p[0].text == "usb" && p[1].text == "Driver")
                    .map(|p| &p[1])
            });
        let Some(usb) = usb else {
            return Vec::new();
        };
        let has = |s: &str| scan::mentions(&words, s);
        // an external ULPI PHY clocks a high-speed OTG itself
        let clock48 = words.iter().any(|w| w.text == "new_hs_ulpi")
            || has("hsi48")
            || has("clk48")
            || has("clk48sel")
            || has("msipll")
            || has("msipllen")
            || (has("hse") && has("pll"));
        if clock48 {
            return Vec::new();
        }
        vec![
            Finding::new(
                "CLK-5",
                "usb-clock",
                "USB is used but no exact 48 MHz source is configured: no HSI48 with CRS, and no HSE through a PLL",
            )
            .span(usb.loc.span().label("USB device"))
            .fix("on parts with HSI48: config.rcc.hsi48 = Some(Hsi48Config { sync_from_usb: true }) and the USB mux on HSI48; without it: an HSE crystal and a PLL output at 48 MHz"),
        ]
    }
}
