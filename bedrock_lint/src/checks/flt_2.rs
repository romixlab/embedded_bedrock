//! `FLT-2` Logging, counting or a possible panic inside HardFault / DefaultHandler

use super::flt;
use crate::check::{Check, CheckMeta, Need, Registered};
use crate::finding::{Confidence, Finding};
use crate::model::Firmware;
use crate::model::scan::{self, Node};
use syn::Expr;
use syn::spanned::Spanned;

pub struct Flt2;

static META: CheckMeta = CheckMeta {
    id: "FLT-2",
    needs: &[Need::Sources],
};

inventory::submit!(Registered(&Flt2));

impl Check for Flt2 {
    fn meta(&self) -> &'static CheckMeta {
        &META
    }

    /// In `HardFault` and `DefaultHandler`: defmt logging (`info!` .. `error!`, `Debug2Format`), `cnt!` counters,
    /// `unwrap` / `expect`, `panic!` and `assert!`. Each can nest into a log that was interrupted or panic inside
    /// the handler (typec_dongle FW-48). In HardFault, after the crash record is saved (`capture_fault`) it is an
    /// estimate: the record is already safe, only the reset is at stake.
    fn run(&self, fw: &Firmware) -> Vec<Finding> {
        let fns = scan::functions(fw);
        let mut out = Vec::new();
        for name in ["HardFault", "DefaultHandler"] {
            let Some(h) = flt::handler(&fns, name) else {
                continue;
            };
            let mut captured: Option<proc_macro2::LineColumn> = None;
            let mut sites: Vec<(proc_macro2::Span, String)> = Vec::new();
            scan::each(h.block, |n| match n {
                Node::Macro(m, mac) if flt::nesting_macro(m) => {
                    sites.push((mac.path.span(), format!("{m}!")));
                }
                Node::Expr(Expr::MethodCall(c)) if c.method == "unwrap" || c.method == "expect" => {
                    sites.push((c.method.span(), format!(".{}()", c.method)));
                }
                Node::Expr(Expr::Path(p))
                    if p.path.segments.last().is_some_and(|s| {
                        s.ident == "Debug2Format" || s.ident == "Display2Format"
                    }) =>
                {
                    sites.push((p.span(), scan::text(p)));
                }
                Node::Expr(e @ Expr::Call(_))
                    if captured.is_none()
                        && scan::call_name(e).is_some_and(|n| n.ends_with("capture_fault")) =>
                {
                    captured = Some(e.span().start());
                }
                _ => {}
            });
            for (span, what) in sites {
                let after_capture = captured.is_some_and(|c| {
                    let s = span.start();
                    (s.line, s.column) > (c.line, c.column)
                });
                let mut f = Finding::new(
                    "FLT-2",
                    format!("{}:{what}", h.path()),
                    format!(
                        "`{what}` in {name}: it can nest into an interrupted log or panic inside the handler (typec_dongle FW-48)"
                    ),
                )
                .span(scan::loc(h.file, span).span().label(format!("{what} in {name}")))
                .fix(if name == "DefaultHandler" {
                    "record the IRQ number raw (a backup register, a static) and log it from the next boot"
                } else {
                    "save the crash record and reset; the next boot logs and counts what the record says"
                });
                if after_capture {
                    f = f.confidence(Confidence::Estimate);
                }
                out.push(f);
            }
        }
        out
    }
}
