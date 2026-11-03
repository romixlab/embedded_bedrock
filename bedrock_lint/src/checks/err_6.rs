//! `ERR-6` RP2350 A2 pad with pull-down and input buffer left on (RP2350-E9)

use crate::check::{Check, CheckMeta, Need, Registered};
use crate::finding::Finding;
use crate::model::Firmware;
use crate::model::scan::{self, Node};
use syn::Expr;

pub struct Err6;

static META: CheckMeta = CheckMeta {
    id: "ERR-6",
    needs: &[Need::Sources],
};

inventory::submit!(Registered(&Err6));

impl Check for Err6 {
    fn meta(&self) -> &'static CheckMeta {
        &META
    }

    /// On RP2350, a pad pull-down (`Pull::Down`, `set_pde(true)`) in a firmware that never switches a pad's input
    /// buffer (`set_ie`). RP2350 datasheet appendix D.5.1, RP2350-E9 "Increased leakage current on Bank 0 GPIO when
    /// pad input is enabled", affects RP2350 A2, fixed by RP2350 A3: "The pad pull-down (if enabled) is
    /// significantly weaker than the leakage current in this state and therefore isn't strong enough to pull the
    /// pad voltage low." Workaround: "clear the pad input enable in GPIO0.IE ... enable the input buffer by setting
    /// GPIO0.IE immediately before reading, and then re-disable immediately afterwards", or "an external pull-down
    /// of 8.2 kΩ or less". A board known to carry A3 or later only is a waiver with that reason.
    fn run(&self, fw: &Firmware) -> Vec<Finding> {
        if !scan::family_in(fw, &["rp235x"]) {
            return Vec::new();
        }
        let fns = scan::functions(fw);
        let toggles_ie = fns.iter().any(|f| {
            scan::method_calls(f)
                .iter()
                .any(|c| c.name == "set_ie" || c.name == "set_input_enable")
        });
        if toggles_ie {
            return Vec::new();
        }
        let mut out = Vec::new();
        for f in &fns {
            scan::each(f.block, |n| {
                let at = match n {
                    Node::Expr(Expr::Path(p)) => {
                        let segs: Vec<String> = p
                            .path
                            .segments
                            .iter()
                            .map(|s| s.ident.to_string())
                            .collect();
                        (segs.len() >= 2
                            && segs[segs.len() - 2] == "Pull"
                            && segs[segs.len() - 1] == "Down")
                            .then(|| scan::loc(f.file, syn::spanned::Spanned::span(p)))
                    }
                    Node::Expr(Expr::MethodCall(m))
                        if m.method == "set_pde"
                            && m.args.first().and_then(scan::bool_value) == Some(true) =>
                    {
                        Some(scan::loc(f.file, m.method.span()))
                    }
                    _ => None,
                };
                if let Some(at) = at {
                    out.push(
                        Finding::new(
                            "ERR-6",
                            format!("{}:pull-down", f.path()),
                            "pad pull-down with the input buffer left on: on RP2350 A2 (RP2350-E9 \"Increased leakage current on Bank 0 GPIO when pad input is enabled\") the leakage holds the pad at about 2.2 V and the pull-down can't pull it low",
                        )
                        .span(at.span())
                        .fix("keep the pad's input enable off and set GPIO.IE only for the read (the datasheet's workaround), or fit an external pull-down of 8.2 kΩ or less"),
                    );
                }
            });
        }
        out
    }
}
