//! `IRQ-8` Unused interrupt vectors not caught: `DefaultHandler` must count the IRQ number (and not log)

use super::util::{is_log_macro, macro_name, walk_macros};
use crate::check::{Check, CheckMeta, Need, Registered};
use crate::finding::{Confidence, Finding};
use crate::model::context::bodies;
use crate::model::{Firmware, HandlerKind};

pub struct Irq8;

static META: CheckMeta = CheckMeta {
    id: "IRQ-8",
    needs: &[Need::Sources],
};

inventory::submit!(Registered(&Irq8));

/// `cnt!` and its backup-register counterpart
fn is_count_macro(name: &str) -> bool {
    matches!(name, "cnt" | "bkp_cnt")
}

impl Check for Irq8 {
    fn meta(&self) -> &'static CheckMeta {
        &META
    }

    fn run(&self, fw: &Firmware) -> Vec<Finding> {
        let facts = &fw.facts;
        let handler = facts
            .handlers
            .iter()
            .find(|h| h.kind == HandlerKind::Exception && h.name == "DefaultHandler");
        let Some(h) = handler else {
            let Some(dep) = fw.dependencies().into_iter().find(|d| d.package() == "cortex-m-rt") else {
                return Vec::new();
            };
            let mut path: Vec<&str> = dep.table.iter().map(String::as_str).collect();
            path.push(&dep.name);
            return vec![
                Finding::new(
                    "IRQ-8",
                    "DefaultHandler",
                    "no #[exception] fn DefaultHandler in the source seen: an interrupt that fires on an unbound \
                     vector goes unnoticed instead of being counted",
                )
                .span(crate::Span::new(dep.file.rel(), dep.file.line_of(&path)))
                .fix("add #[exception] fn DefaultHandler(irqn: i16) that counts irqn with cnt! and does not log")
                .confidence(Confidence::Estimate),
            ];
        };
        let bodies = bodies(&fw.sources);
        let Some(fb) = bodies.iter().find(|b| b.path == h.path) else {
            return Vec::new();
        };
        let mut out = Vec::new();
        let mut counted = false;
        let mut logged: Option<(String, crate::Span)> = None;
        walk_macros(fb.block, &mut |m| {
            let name = macro_name(m);
            if is_count_macro(&name) {
                counted = true;
            }
            if logged.is_none() && is_log_macro(&name) {
                logged = Some((name, fb.loc(syn::spanned::Spanned::span(&m.path)).span()));
            }
        });
        if !counted {
            out.push(
                Finding::new(
                    "IRQ-8",
                    format!("{}:no-count", h.path),
                    "DefaultHandler does not count the IRQ number: an unbound vector that fires goes unnoticed",
                )
                .span(h.loc.span())
                .fix("count irqn with cnt! (and bkp_cnt! if it should survive a reset)"),
            );
        }
        if let Some((name, at)) = logged {
            out.push(
                Finding::new(
                    "IRQ-8",
                    format!("{}:logs", h.path),
                    format!(
                        "DefaultHandler logs ({name}!): the catalogue asks it to count and not log, since the \
                         context it fires in is not safe to format or send in"
                    ),
                )
                .span(at)
                .fix("drop the log call; count irqn with cnt! instead"),
            );
        }
        out
    }
}
