//! `USB-1` High-speed bulk endpoint max packet not 512, bInterval out of range for the speed

use crate::check::{Check, CheckMeta, Need, Registered};
use crate::finding::Finding;
use crate::model::Firmware;
use crate::model::scan;

pub struct Usb1;

static META: CheckMeta = CheckMeta {
    id: "USB-1",
    needs: &[Need::Sources],
};

inventory::submit!(Registered(&Usb1));

/// Words that make the device high speed: embassy-stm32's ULPI driver, embassy-usb's speed
const HIGH_SPEED: &[&str] = &["new_hs_ulpi", "new_hs"];

impl Check for Usb1 {
    fn meta(&self) -> &'static CheckMeta {
        &META
    }

    /// embassy-usb endpoints with literal sizes (`endpoint_bulk_in(None, 64)`, constants resolved): on a
    /// high-speed device (`Driver::new_hs_ulpi`) a bulk endpoint's max packet must be 512 (USB 2.0 allows no other
    /// size there; Windows refuses the device otherwise, vb135 c0d19b8). An interrupt endpoint's bInterval must be 1..=16 at high speed
    /// (2^(n-1) microframes) and 1..=255 at full speed; 0 is never valid.
    fn run(&self, fw: &Firmware) -> Vec<Finding> {
        let words = scan::words(fw);
        let high = words.iter().any(|w| HIGH_SPEED.contains(&w.text.as_str()));
        let consts = scan::consts(fw);
        let mut out = Vec::new();
        for f in scan::functions(fw) {
            for c in scan::method_calls(&f) {
                let name = c.name.as_str();
                let mps = c.args.get(1).and_then(|a| scan::int_value(a, &consts));
                if name.starts_with("endpoint_bulk") && high && mps.is_some_and(|m| m != 512) {
                    out.push(
                        Finding::new(
                            "USB-1",
                            format!("{}:{name}", f.path()),
                            format!(
                                "bulk endpoint with max packet {} on a high-speed device: high speed allows only 512, and Windows refuses the configuration",
                                mps.unwrap_or_default()
                            ),
                        )
                        .span(c.loc.span())
                        .fix("use 512 at high speed (64 at full speed), from the speed the driver reports"),
                    );
                }
                if name.starts_with("endpoint_interrupt")
                    && let Some(interval) = c.args.get(2).and_then(|a| scan::int_value(a, &consts))
                {
                    let max = if high { 16 } else { 255 };
                    if interval == 0 || interval > max {
                        out.push(
                            Finding::new(
                                "USB-1",
                                format!("{}:{name}:interval", f.path()),
                                format!(
                                    "interrupt endpoint bInterval {interval} out of range at {} speed (1..={max})",
                                    if high { "high" } else { "full" }
                                ),
                            )
                            .span(c.loc.span())
                            .fix(if high {
                                "at high speed bInterval is an exponent: 2^(n-1) microframes, 1..=16"
                            } else {
                                "at full speed bInterval is in frames, 1..=255"
                            }),
                        );
                    }
                }
            }
        }
        out
    }
}
