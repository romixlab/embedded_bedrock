//! `PIN-4` RP peripheral pins moved to GPIO and back under an enabled peripheral

use super::pin;
use crate::check::{Check, CheckMeta, Need, Registered};
use crate::finding::{Confidence, Finding};
use crate::model::Firmware;
use crate::model::scan::{self, Node};
use syn::Expr;

pub struct Pin4;

static META: CheckMeta = CheckMeta {
    id: "PIN-4",
    needs: &[Need::Sources],
};

inventory::submit!(Registered(&Pin4));

const FIX: &str = "leave FUNCSEL on the peripheral and drive the pad with OEOVER / OUTOVER (IO_BANK0 GPIOx_CTRL), or disable the peripheral before the pins change hands";

impl Check for Pin4 {
    fn meta(&self) -> &'static CheckMeta {
        &META
    }

    /// On RP2040 / RP235x: a FUNCSEL write (`set_funcsel(..)`) outside a function named `new` / `init` / `setup`, and
    /// not beside `set_oeover` / `set_outover`, is an estimate; a GPIO driver built on a pin taken with `steal()` (the peripheral's pin borrowed behind its back)
    /// is a candidate. The RP2350 I2C block reads the OR of every pad with its FUNCSEL, so a pin switched to GPIO
    /// and back showed it a start without a stop (usb_io_fw IF-2).
    fn run(&self, fw: &Firmware) -> Vec<Finding> {
        if !scan::family(fw).is_some_and(|f| f.starts_with("rp")) {
            return Vec::new();
        }
        let mut out = Vec::new();
        for f in scan::functions(fw) {
            let path = f.path();
            let name = f.name();
            let mut overrides = false;
            scan::each(f.block, |n| {
                if let Node::Expr(Expr::MethodCall(m)) = n
                    && (m.method == "set_oeover" || m.method == "set_outover")
                {
                    overrides = true;
                }
            });
            // FUNCSEL written with the overrides is the way this check asks for
            let setup = overrides
                || ["new", "init", "setup"]
                    .iter()
                    .any(|w| name == *w || name.starts_with(&format!("{w}_")));
            scan::each(f.block, |n| match n {
                Node::Expr(Expr::MethodCall(m)) if m.method == "set_funcsel" && !setup => {
                    out.push(
                        Finding::new("PIN-4", format!("{path}:funcsel"), format!("`{path}` switches a pin's FUNCSEL at run time: the peripheral it leaves still sees the pad and may take the glitch as bus traffic"))
                            .span(scan::loc(f.file, m.method.span()).span().label("FUNCSEL changed"))
                            .fix(FIX)
                            .confidence(Confidence::Estimate),
                    );
                }
                Node::Expr(e @ Expr::Call(c)) => {
                    if let Some(ty) = pin::driver_new(e)
                        && c.args.iter().any(|a| scan::text(a).contains("steal"))
                    {
                        out.push(
                            Finding::new("PIN-4", format!("{path}:steal:{ty}"), format!("`{path}` builds a {ty} on a stolen pin: if a peripheral owns it, its FUNCSEL moves to GPIO under it"))
                                .span(scan::loc(f.file, scan::head_span(e)).span().label("GPIO on a stolen pin"))
                                .fix(FIX)
                                .confidence(Confidence::Candidate),
                        );
                    }
                }
                _ => {}
            });
        }
        out
    }
}
