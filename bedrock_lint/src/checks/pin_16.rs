//! `PIN-16` G0 pin read that a dead-battery pull-down still holds

use crate::check::{Check, CheckMeta, Need, Registered};
use crate::finding::{Confidence, Finding};
use crate::model::Firmware;
use crate::model::scan::{self, Node};
use syn::Expr;

pub struct Pin16;

static META: CheckMeta = CheckMeta {
    id: "PIN-16",
    needs: &[Need::Sources],
};

inventory::submit!(Registered(&Pin16));

impl Check for Pin16 {
    fn meta(&self) -> &'static CheckMeta {
        &META
    }

    /// On STM32G0: a `set_ucpdN_strobe(false)` hands the UCPD pins back to the dead-battery pull-down (certain). A
    /// pin read (`is_high()`, `is_low()`, `get_level()`) before the strobe is set in the same function is an
    /// estimate. Reads in a G0 firmware that never sets a strobe and runs no UCPD driver are one candidate: a read on
    /// a DBCC pin sees the pull-down (typec_dongle FW-22: PA10, UCPD1's DBCC2, read low on every board).
    fn run(&self, fw: &Firmware) -> Vec<Finding> {
        if scan::family(fw).as_deref() != Some("stm32g0") {
            return Vec::new();
        }
        let idents = scan::code_idents(fw);
        let ucpd_driver = idents
            .windows(2)
            .any(|w| w[0].name == "Ucpd" && w[1].name == "new");
        let fns = scan::functions(fw);
        let mut out = Vec::new();
        let mut any_strobe = false;
        let mut first_read: Option<(crate::model::Loc, String)> = None;
        for f in &fns {
            let path = f.path();
            let mut strobe_at: Option<proc_macro2::LineColumn> = None;
            let mut reads: Vec<proc_macro2::Span> = Vec::new();
            scan::each(f.block, |n| {
                let Node::Expr(Expr::MethodCall(m)) = n else {
                    return;
                };
                let name = m.method.to_string();
                if name.starts_with("set_ucpd") && name.ends_with("_strobe") {
                    let on = m.args.first().map(scan::text);
                    if on.as_deref() == Some("false") {
                        out.push(
                            Finding::new("PIN-16", format!("{path}:{name}:false"), format!("`{name}(false)` in `{path}` gives the UCPD pins back to the dead-battery pull-down"))
                                .span(scan::loc(f.file, m.method.span()).span().label("strobe cleared"))
                                .fix("set the strobe once at boot and never clear it"),
                        );
                    } else {
                        any_strobe = true;
                        strobe_at.get_or_insert(m.method.span().start());
                    }
                } else if matches!(name.as_str(), "is_high" | "is_low" | "get_level") {
                    reads.push(m.method.span());
                }
            });
            if first_read.is_none()
                && let Some(r) = reads.first()
            {
                first_read = Some((scan::loc(f.file, *r), path.clone()));
            }
            if let Some(s) = strobe_at
                && let Some(r) = reads.iter().find(|r| {
                    let p = r.start();
                    (p.line, p.column) < (s.line, s.column)
                })
            {
                out.push(
                    Finding::new("PIN-16", format!("{path}:read-before-strobe"), format!("`{path}` reads a pin before it sets the UCPD strobe: a DBCC pin still sees the dead-battery pull-down"))
                        .span(scan::loc(f.file, *r).span().label("read here"))
                        .fix("set `SYSCFG.cfgr1().ucpdN_strobe` before reading the pin")
                        .confidence(Confidence::Estimate),
                );
            }
        }
        if !any_strobe
            && !ucpd_driver
            && let Some((loc, path)) = first_read
        {
            out.push(
                Finding::new("PIN-16", "no-strobe", format!("pins are read (first in `{path}`) and the UCPD strobe is never set: on a DBCC pin the read sees the dead-battery pull-down"))
                    .span(loc.span().label("read here"))
                    .fix("check the pins read against the UCPD dead-battery pins of the part; on one, set `SYSCFG.cfgr1().ucpdN_strobe` first")
                    .confidence(Confidence::Candidate),
            );
        }
        out
    }
}
