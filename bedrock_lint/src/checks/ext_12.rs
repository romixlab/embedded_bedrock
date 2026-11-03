//! `EXT-12` RCC enable not read back before the first access to the peripheral

use crate::check::{Check, CheckMeta, Need, Registered};
use crate::finding::Finding;
use crate::model::Firmware;
use crate::model::scan;

pub struct Ext12;

static META: CheckMeta = CheckMeta {
    id: "EXT-12",
    needs: &[Need::Sources],
};

inventory::submit!(Registered(&Ext12));

impl Check for Ext12 {
    fn meta(&self) -> &'static CheckMeta {
        &META
    }

    /// On STM32, in one function: a write of an RCC enable register (`rcc.apbenr1().modify(..)`, a receiver naming
    /// RCC and a register with `enr` in it) followed by an access to another peripheral's register before any read
    /// of an RCC enable register: the enable takes a few bus cycles to arrive, and the access goes to a peripheral
    /// that is still off (the template's `src/init.rs` reads it back).
    fn run(&self, fw: &Firmware) -> Vec<Finding> {
        if !scan::is_stm32(fw) {
            return Vec::new();
        }
        let mut out = Vec::new();
        for f in scan::functions(fw) {
            let mut pending: Option<scan::MethodCall> = None;
            for c in scan::method_calls(&f) {
                if !matches!(c.name.as_str(), "modify" | "write" | "write_value" | "read") {
                    continue;
                }
                let r = c.receiver.to_ascii_lowercase();
                let rcc_enr = r.contains("rcc")
                    && r.rsplit('.').next().is_some_and(|reg| reg.contains("enr"));
                if rcc_enr {
                    if c.name == "read" {
                        pending = None;
                    } else {
                        pending = Some(c);
                    }
                    continue;
                }
                if r.contains("rcc") {
                    continue;
                }
                if let Some(en) = pending.take() {
                    out.push(
                        Finding::new(
                            "EXT-12",
                            format!("{}:{}", f.path(), c.receiver),
                            format!(
                                "{} is accessed right after its clock is enabled in {}: the enable may not have reached it yet",
                                c.receiver, en.receiver
                            ),
                        )
                        .span(c.loc.span().label("first access"))
                        .span(en.loc.span().label("clock enabled"))
                        .fix(format!("read the enable register back first: `let _ = {}.read();`", en.receiver)),
                    );
                }
            }
        }
        out
    }
}
