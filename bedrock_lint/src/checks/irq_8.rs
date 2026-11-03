//! `IRQ-8` Unused interrupt vectors not caught: `DefaultHandler` must record the IRQ number

use super::util::{is_log_macro, macro_name, walk_macros};
use crate::check::{Check, CheckMeta, Need, Registered};
use crate::finding::{Confidence, Finding};
use crate::model::context::bodies;
use crate::model::{Firmware, HandlerKind};
use syn::Token;
use syn::punctuated::Punctuated;
use syn::visit::Visit;

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

/// Whether a counter macro is in the block
fn counted_by_macro(block: &syn::Block) -> bool {
    let mut found = false;
    walk_macros(block, &mut |m| found |= is_count_macro(&macro_name(m)));
    found
}

/// Whether the parameter is named outside the bodies of log macros
fn used_outside_logs(block: &syn::Block, param: &str) -> bool {
    struct V<'p> {
        param: &'p str,
        found: bool,
    }
    impl<'ast> Visit<'ast> for V<'_> {
        fn visit_expr_path(&mut self, p: &'ast syn::ExprPath) {
            self.found |= p.path.is_ident(self.param);
        }
        fn visit_macro(&mut self, m: &'ast syn::Macro) {
            if is_log_macro(&macro_name(m)) {
                return;
            }
            if let Ok(args) =
                m.parse_body_with(Punctuated::<syn::Expr, Token![,]>::parse_terminated)
            {
                for a in &args {
                    self.visit_expr(a);
                }
            }
        }
    }
    let mut v = V {
        param,
        found: false,
    };
    v.visit_block(block);
    v.found
}

/// The name of the handler's IRQ number parameter (`irqn`)
fn h_param(sig: &syn::Signature) -> Option<String> {
    match sig.inputs.first()? {
        syn::FnArg::Typed(t) => match &*t.pat {
            syn::Pat::Ident(i) => Some(i.ident.to_string()),
            _ => None,
        },
        syn::FnArg::Receiver(_) => None,
    }
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
            let Some(dep) = fw
                .dependencies()
                .into_iter()
                .find(|d| d.package() == "cortex-m-rt")
            else {
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
                .fix("add #[exception] fn DefaultHandler(irqn: i16) that records irqn raw (a static, the crash record or cnt!)")
                .confidence(Confidence::Estimate),
            ];
        };
        let bodies = bodies(&fw.sources);
        let Some(fb) = bodies.iter().find(|b| b.path == h.path) else {
            return Vec::new();
        };
        // the IRQ number recorded: a counter macro, or the parameter used outside a log call (stored raw, as FLT-2
        // asks); logging inside the handler is FLT-2's finding, not this one's
        let recorded = counted_by_macro(fb.block)
            || h_param(fb.sig).is_some_and(|p| used_outside_logs(fb.block, &p));
        let mut out = Vec::new();
        if !recorded {
            out.push(
                Finding::new(
                    "IRQ-8",
                    format!("{}:no-record", h.path),
                    "DefaultHandler does not record the IRQ number: an unbound vector that fires goes unnoticed",
                )
                .span(h.loc.span())
                .fix("record irqn raw: store it in a static or the crash record, or count it with cnt!"),
            );
        }
        out
    }
}
