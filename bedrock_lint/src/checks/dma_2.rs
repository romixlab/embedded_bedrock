//! `DMA-2` D-cache on with DMA buffers in cacheable RAM and no clean / invalidate or MPU non-cacheable region

use super::util::{names_any, walk_exprs};
use crate::check::{Check, CheckMeta, Need, Registered};
use crate::finding::{Confidence, Finding};
use crate::model::Firmware;
use crate::model::context::bodies;
use syn::Expr;
use syn::spanned::Spanned;

pub struct Dma2;

static META: CheckMeta = CheckMeta {
    id: "DMA-2",
    needs: &[Need::Sources, Need::Chip],
};

inventory::submit!(Registered(&Dma2));

/// The last segment of a path or method call's name
fn call_name(e: &Expr) -> Option<String> {
    match e {
        Expr::Call(c) => match &*c.func {
            Expr::Path(p) => p.path.segments.last().map(|s| s.ident.to_string()),
            _ => None,
        },
        Expr::MethodCall(m) => Some(m.method.to_string()),
        _ => None,
    }
}

impl Check for Dma2 {
    fn meta(&self) -> &'static CheckMeta {
        &META
    }

    /// A Cortex-M7 part (STM32H7 / F7) whose source calls `enable_dcache` and uses a `DMA` peripheral, with no
    /// `clean_dcache` / `invalidate_dcache` call and no MPU setup anywhere in the source seen
    fn run(&self, fw: &Firmware) -> Vec<Finding> {
        let Some(chip) = fw.chip() else {
            return Vec::new();
        };
        if !matches!(chip.family.as_str(), "stm32h7" | "stm32f7") {
            return Vec::new();
        }
        if !fw.facts.peripherals.iter().any(|p| p.name.contains("DMA")) {
            return Vec::new();
        }
        let mpu_used = fw
            .sources
            .iter()
            .any(|s| names_any(&s.ast, &["MPU", "Mpu"]));
        let mut enable_at = None;
        let mut maintained = false;
        for fb in bodies(&fw.sources) {
            walk_exprs(fb.block, &mut |e| {
                let Some(name) = call_name(e) else { return };
                match name.as_str() {
                    "enable_dcache" if enable_at.is_none() => {
                        let span = match e {
                            Expr::Call(c) => c.func.span(),
                            Expr::MethodCall(m) => m.method.span(),
                            _ => return,
                        };
                        enable_at = Some(fb.loc(span));
                    }
                    "clean_dcache" | "invalidate_dcache" | "clean_invalidate_dcache" => {
                        maintained = true;
                    }
                    _ => {}
                }
            });
        }
        let Some(at) = enable_at else {
            return Vec::new();
        };
        if maintained || mpu_used {
            return Vec::new();
        }
        vec![
            Finding::new(
                "DMA-2",
                "enable_dcache",
                "D-cache is enabled and DMA is in use, with no clean_dcache / invalidate_dcache call and no MPU \
                 setup in the source seen: a DMA buffer in cacheable RAM can read or write stale data",
            )
            .span(at.span().label("enable_dcache()"))
            .fix(
                "clean the buffer before a DMA write, invalidate it before a DMA read, or put DMA buffers in an \
                 MPU region marked non-cacheable",
            )
            .confidence(Confidence::Estimate),
        ]
    }
}
