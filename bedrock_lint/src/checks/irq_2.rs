//! `IRQ-2` `rc_w1` / W1C flags cleared with `modify()` instead of a write of the one bit

use super::util::{names_any, text, walk_exprs};
use crate::check::{Check, CheckMeta, Need, Registered};
use crate::finding::{Confidence, Finding};
use crate::model::Firmware;
use crate::model::context::bodies;
use syn::Expr;

pub struct Irq2;

static META: CheckMeta = CheckMeta {
    id: "IRQ-2",
    needs: &[Need::Sources],
};

inventory::submit!(Registered(&Irq2));

/// Receiver-chain names that read as a status / flag register, as the catalogue lists them
const REGISTER_NAMES: &[&str] = &["isr", "sr", "ir", "ifr"];

impl Check for Irq2 {
    fn meta(&self) -> &'static CheckMeta {
        &META
    }

    /// `<reg>.modify(|w| ..)` where the receiver chain names a status/flag register: a read-modify-write can clear
    /// a flag that was set between the read and the write, as embassy-stm32 0.6's G0 ADC ISR did to CCRDY. No SVD
    /// access types yet to tell a genuine R/W field from a `rc_w1` one, so this is always a candidate.
    fn run(&self, fw: &Firmware) -> Vec<Finding> {
        let mut out = Vec::new();
        for fb in bodies(&fw.sources) {
            walk_exprs(fb.block, &mut |e| {
                let Expr::MethodCall(m) = e else { return };
                if m.method != "modify"
                    || m.args.len() != 1
                    || !matches!(&m.args[0], Expr::Closure(_))
                    || !names_any(&m.receiver, REGISTER_NAMES)
                {
                    return;
                }
                let loc = fb.loc(m.method.span());
                out.push(
                    Finding::new(
                        "IRQ-2",
                        format!("{}:{}", fb.path, loc.line),
                        format!(
                            "{}.modify() reads, changes and writes back the whole register: a flag of it that was \
                             set between the read and the write is cleared unseen",
                            text(&m.receiver)
                        ),
                    )
                    .span(loc.span().label(format!("{}.modify(..)", text(&m.receiver))))
                    .confidence(Confidence::Candidate),
                );
            });
        }
        out
    }
}
