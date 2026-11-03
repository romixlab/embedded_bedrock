//! `ERR-5` FDCAN DAR (one-shot) without retransmit after a lost arbitration on the first two ID bits

use crate::check::{Check, CheckMeta, Need, Registered};
use crate::finding::{Confidence, Finding};
use crate::model::Firmware;
use crate::model::scan::{self, Node};
use syn::Expr;

pub struct Err5;

static META: CheckMeta = CheckMeta {
    id: "ERR-5",
    needs: &[Need::Sources],
};

inventory::submit!(Registered(&Err5));

impl Check for Err5 {
    fn meta(&self) -> &'static CheckMeta {
        &META
    }

    /// On H7, DAR turned on (`set_dar(true)`, `set_automatic_retransmit(false)`, `dar: true`) in a firmware that
    /// never handles the erratum's case (no TXBCF / TXBRP access). ES0491 Rev 9
    /// 2.22.3 "DAR mode transmission failure due to lost arbitration": "In DAR mode, the transmission may fail due to
    /// lost arbitration at the first two identifier bits"; workaround "clear the corresponding Tx buffer
    /// transmission request bit TRPx of the FDCAN_TXBRP register and set the corresponding cancellation finished bit
    /// CFx of the FDCAN_TXBCF register, then restart the transmission" (quoted in
    /// firmware_template_skill/references/errata.md). A literal is certain; a setter given a run-time value (a
    /// host option) is a candidate.
    fn run(&self, fw: &Firmware) -> Vec<Finding> {
        if !scan::family_in(fw, &["stm32h7"]) {
            return Vec::new();
        }
        let words = scan::words(fw);
        let handled = words.iter().any(|w| w.has("txbcf") || w.has("txbrp"));
        if handled {
            return Vec::new();
        }
        let mut out = Vec::new();
        for f in scan::functions(fw) {
            let mut dar: Vec<(crate::model::Loc, Option<bool>)> = Vec::new();
            for c in scan::method_calls(&f) {
                let v = c.args.first().map(scan::bool_value);
                match (c.name.as_str(), v) {
                    ("set_dar", Some(v)) => dar.push((c.loc, v)),
                    ("set_automatic_retransmit", Some(v)) => dar.push((c.loc, v.map(|b| !b))),
                    _ => {}
                }
            }
            scan::each(f.block, |n| {
                if let Node::Expr(Expr::Struct(s)) = n {
                    for fv in &s.fields {
                        let m = scan::text(&fv.member);
                        // the register field only: `one_shot: true` in bedrock_can's Capabilities says it is
                        // supported, not that it is on
                        if m == "dar" && scan::bool_value(&fv.expr) == Some(true) {
                            dar.push((
                                scan::loc(f.file, syn::spanned::Spanned::span(fv)),
                                Some(true),
                            ));
                        }
                    }
                }
            });
            for (loc, v) in dar {
                if v == Some(false) {
                    continue;
                }
                let mut finding = Finding::new(
                    "ERR-5",
                    format!("{}:dar", f.path()),
                    "FDCAN DAR (one-shot) without the ES0491 2.22.3 handling: \"In DAR mode, the transmission may fail due to lost arbitration at the first two identifier bits\", and the frame is dropped",
                )
                .span(loc.span())
                .fix("after a lost arbitration in DAR mode clear TRPx in FDCAN_TXBRP, set CFx in FDCAN_TXBCF and send the frame again (the erratum's workaround)");
                if v.is_none() {
                    finding = finding.confidence(Confidence::Candidate);
                }
                out.push(finding);
            }
        }
        out
    }
}
