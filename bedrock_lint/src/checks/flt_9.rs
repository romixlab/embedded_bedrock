//! `FLT-9` Healthy mark never set on the transport in use

use crate::check::{Check, CheckMeta, Need, Registered};
use crate::finding::Finding;
use crate::model::Firmware;
use crate::model::scan;

pub struct Flt9;

static META: CheckMeta = CheckMeta {
    id: "FLT-9",
    needs: &[Need::Sources],
};

inventory::submit!(Registered(&Flt9));

impl Check for Flt9 {
    fn meta(&self) -> &'static CheckMeta {
        &META
    }

    /// The firmware waits for a healthy mark (`healthy_watch`) and runs a USB device (`embassy_usb`, `UsbDevice`),
    /// but never calls `usb_configured()`: every boot waits the whole timeout before it counts as healthy, and a
    /// few quick re-flashes land in the ROM bootloader (embedded_bedrock TPL-64).
    fn run(&self, fw: &Firmware) -> Vec<Finding> {
        let idents = scan::code_idents(fw);
        let Some(watch) = scan::first_ident(&idents, &["healthy_watch"]) else {
            return Vec::new();
        };
        let usb = scan::first_ident(&idents, &["embassy_usb", "UsbDevice", "usb_device"]).is_some();
        if !usb || scan::first_ident(&idents, &["usb_configured"]).is_some() {
            return Vec::new();
        }
        vec![
            Finding::new(
                "FLT-9",
                "usb-configured",
                "the boot waits for a healthy mark and the firmware runs USB, but `usb_configured()` is never called: every boot waits the whole timeout",
            )
            .span(watch.loc.span().label("healthy_watch"))
            .fix("call `bedrock_rom_boot::usb_configured()` when the host configures the device (the USB `Handler::configured` callback)"),
        ]
    }
}
