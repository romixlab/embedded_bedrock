//! `DMA-4` DMA ring overrun not counted: every ring consumer counts overruns with `cnt!`

use super::util::{macro_name, names_any, walk_exprs, walk_macros};
use crate::check::{Check, CheckMeta, Need, Registered};
use crate::finding::{Confidence, Finding};
use crate::model::Firmware;
use crate::model::context::{FnBody, bodies};
use syn::Expr;

pub struct Dma4;

static META: CheckMeta = CheckMeta {
    id: "DMA-4",
    needs: &[Need::Sources],
};

inventory::submit!(Registered(&Dma4));

/// Types a DMA ring consumer is bound to, in a parameter, a local or a receiver's chain
const RING_TYPES: &[&str] = &["RingBufferedUartRx", "ReadableDmaRingBuffer"];

/// Whether `fb` names a DMA ring consumer: `into_ring_buffered(..)` called in it, or one of [`RING_TYPES`] named in
/// its signature or body
fn uses_ring(fb: &FnBody) -> bool {
    let mut called = false;
    walk_exprs(fb.block, &mut |e| {
        if let Expr::MethodCall(m) = e
            && m.method == "into_ring_buffered"
        {
            called = true;
        }
    });
    called || names_any(fb.sig, RING_TYPES) || names_any(fb.block, RING_TYPES)
}

/// `cnt!` / `bkp_cnt!` anywhere in the body
fn is_counted(block: &syn::Block) -> bool {
    let mut counted = false;
    walk_macros(block, &mut |m| {
        if matches!(macro_name(m).as_str(), "cnt" | "bkp_cnt") {
            counted = true;
        }
    });
    counted
}

impl Check for Dma4 {
    fn meta(&self) -> &'static CheckMeta {
        &META
    }

    /// A function that uses a DMA ring consumer with no `cnt!` / `bkp_cnt!` call anywhere in it: an overrun on the
    /// ring (typec_dongle FW-26: `adc_overruns`, `uart_rx_errors`, both counted) goes unnoticed. Same-function is
    /// the main case this looks at, like IRQ-6's disable/enable pair.
    fn run(&self, fw: &Firmware) -> Vec<Finding> {
        let mut out = Vec::new();
        for fb in bodies(&fw.sources) {
            if !uses_ring(&fb) || is_counted(fb.block) {
                continue;
            }
            let loc = fb.loc(fb.sig.ident.span());
            out.push(
                Finding::new(
                    "DMA-4",
                    fb.path.clone(),
                    format!(
                        "{} uses a DMA ring consumer with no cnt! / bkp_cnt! call anywhere in it: an overrun on \
                         the ring goes uncounted and unnoticed",
                        fb.path
                    ),
                )
                .span(loc.span().label(format!("fn {}", fb.sig.ident)))
                .fix("count the ring's overrun / Err case with cnt! (and bkp_cnt! if it should survive a reset)")
                .confidence(Confidence::Estimate),
            );
        }
        out
    }
}
