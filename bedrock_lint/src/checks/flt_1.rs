//! `FLT-1` HardFault handler that spins in a shipped build

use super::flt;
use crate::check::{Check, CheckMeta, Need, Registered};
use crate::finding::Finding;
use crate::model::Firmware;
use crate::model::scan::{self, Node};
use syn::Expr;

pub struct Flt1;

static META: CheckMeta = CheckMeta {
    id: "FLT-1",
    needs: &[Need::Sources],
};

inventory::submit!(Registered(&Flt1));

const FIX: &str = "save a crash record (bedrock_crash::capture_fault) and reset (bedrock_crash::reset(), SCB::sys_reset()) instead of spinning";

/// Calls that reset the MCU
const RESETS: &[&str] = &[
    "sys_reset",
    "reset",
    "system_reset",
    "reboot",
    "software_reset",
];

impl Check for Flt1 {
    fn meta(&self) -> &'static CheckMeta {
        &META
    }

    /// A Cortex-M firmware without a `HardFault` handler gets cortex-m-rt's, which spins; one whose handler has a
    /// `loop` and calls nothing that resets (`sys_reset`, `bedrock_crash::reset`, `reboot`) spins too. A loop under
    /// `cfg!(debug_assertions)` is fine.
    fn run(&self, fw: &Firmware) -> Vec<Finding> {
        if !scan::cortex_m(fw) {
            return Vec::new();
        }
        let fns = scan::functions(fw);
        let Some(h) = flt::handler(&fns, "HardFault") else {
            return vec![
                Finding::new(
                    "FLT-1",
                    "no-handler",
                    "no HardFault handler: cortex-m-rt's default spins in `loop {}`, so a fault hangs the board until someone power-cycles it",
                )
                .span(scan::entry_span(fw))
                .fix(FIX),
            ];
        };
        let mut spin = None;
        let mut resets = false;
        let mut debug_only = false;
        scan::each(h.block, |n| match n {
            Node::Expr(Expr::Loop(l)) if spin.is_none() => spin = Some(l.loop_token.span),
            Node::Expr(e @ (Expr::Call(_) | Expr::MethodCall(_))) => {
                if let Some(name) = scan::call_name(e) {
                    let last = name
                        .trim_start_matches('.')
                        .rsplit("::")
                        .next()
                        .unwrap_or("");
                    if RESETS.contains(&last) {
                        resets = true;
                    }
                }
            }
            Node::Macro("cfg", m) if m.tokens.to_string().contains("debug_assertions") => {
                debug_only = true
            }
            _ => {}
        });
        match spin {
            Some(span) if !resets && !debug_only => vec![
                Finding::new(
                    "FLT-1",
                    h.path(),
                    "the HardFault handler spins: a fault hangs the board instead of resetting it",
                )
                .span(scan::loc(h.file, span).span().label("spins here"))
                .fix(FIX),
            ],
            _ => Vec::new(),
        }
    }
}
