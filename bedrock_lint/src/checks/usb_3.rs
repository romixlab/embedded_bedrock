//! `USB-3` Single-buffered bulk endpoints on a hot path behind a hub TT

use crate::check::{Check, CheckMeta, Need, Registered};
use crate::finding::Finding;
use crate::model::Firmware;
use crate::model::scan;

pub struct Usb3;

static META: CheckMeta = CheckMeta {
    id: "USB-3",
    needs: &[Need::Sources],
};

inventory::submit!(Registered(&Usb3));

impl Check for Usb3 {
    fn meta(&self) -> &'static CheckMeta {
        &META
    }

    /// On the G0 (STM32 usb_v4, the part it was measured on), an embassy-usb bulk endpoint made with
    /// `endpoint_bulk_in` / `endpoint_bulk_out` instead of the `_double_buffered` variants: behind a hub's
    /// transaction translator the peripheral NAKs while one buffer is processed (typec_dongle FW-24: 500 vs
    /// 947 kB/s). Endpoints off the hot path (a control-like side channel) take a waiver.
    fn run(&self, fw: &Firmware) -> Vec<Finding> {
        if !scan::family_in(fw, &["stm32g0"]) {
            return Vec::new();
        }
        let mut out = Vec::new();
        for f in scan::functions(fw) {
            for c in scan::method_calls(&f) {
                if c.name == "endpoint_bulk_in" || c.name == "endpoint_bulk_out" {
                    out.push(
                        Finding::new(
                            "USB-3",
                            format!("{}:{}", f.path(), c.name),
                            "single-buffered bulk endpoint on usb_v4: behind a hub's transaction translator the device NAKs while the one buffer is processed, about half the throughput",
                        )
                        .span(c.loc.span())
                        .fix(format!("use {}_double_buffered on a hot path", c.name)),
                    );
                }
            }
        }
        out
    }
}
