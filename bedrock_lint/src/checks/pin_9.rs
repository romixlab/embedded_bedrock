//! `PIN-9` Pin map chosen by a cargo feature for a board variant

use super::pin;
use crate::check::{Check, CheckMeta, Need, Registered};
use crate::finding::{Confidence, Finding};
use crate::model::Firmware;
use crate::model::scan;
use quote::ToTokens;
use syn::spanned::Spanned;
use syn::visit::{self, Visit};

pub struct Pin9;

static META: CheckMeta = CheckMeta {
    id: "PIN-9",
    needs: &[Need::Sources],
};

inventory::submit!(Registered(&Pin9));

impl Check for Pin9 {
    fn meta(&self) -> &'static CheckMeta {
        &META
    }

    /// An item, statement, match arm, field or expression under `#[cfg(feature = "..")]` (or the branch of an `if
    /// cfg!(feature = "..")`) that names a pin (`PA5`, `PIN_12`, `P0_13`): a build for one variant flashed onto the
    /// other drives the wrong lines. An estimate: the feature may pick something else than the board.
    fn run(&self, fw: &Firmware) -> Vec<Finding> {
        let mut out = Vec::new();
        for file in &fw.sources {
            let mut v = Gated { found: Vec::new() };
            v.visit_file(&file.ast);
            for (span, feature, pins) in v.found {
                out.push(
                    Finding::new(
                        "PIN-9",
                        format!("{}:{feature}:{}", file.rel, pins.join(",")),
                        format!("feature `{feature}` picks pins ({}): a build for one board variant flashed onto another drives the wrong lines", pins.join(", ")),
                    )
                    .span(scan::loc(file, span).span().label(format!("cfg(feature = \"{feature}\")")))
                    .fix("read the board record (or a strap) at run time and pick the pin map from it; until it is known, leave the pins as inputs")
                    .confidence(Confidence::Estimate),
                );
            }
        }
        out
    }
}

struct Gated {
    found: Vec<(proc_macro2::Span, String, Vec<String>)>,
}

/// The feature a `cfg(..)` attribute or `cfg!(..)` names (`feature = "rev_b"`), also inside `any` / `all` / `not`
fn feature_of(tokens: &str) -> Option<String> {
    let i = tokens.find("feature")?;
    let rest = &tokens[i..];
    let q = rest.find('"')?;
    let end = rest[q + 1..].find('"')?;
    Some(rest[q + 1..q + 1 + end].to_string())
}

fn pins_in(t: impl ToTokens) -> Vec<String> {
    let mut out = Vec::new();
    fn walk(ts: proc_macro2::TokenStream, out: &mut Vec<String>) {
        for t in ts {
            match t {
                proc_macro2::TokenTree::Ident(i) => {
                    let s = i.to_string();
                    if pin::is_pin_name(&s) && !out.contains(&s) {
                        out.push(s);
                    }
                }
                proc_macro2::TokenTree::Group(g) => walk(g.stream(), out),
                _ => {}
            }
        }
    }
    walk(t.to_token_stream(), &mut out);
    out
}

impl Gated {
    /// Returns true when it reported, so the parts are not reported again
    fn check(&mut self, attrs: &[syn::Attribute], node: impl ToTokens) -> bool {
        for a in attrs {
            if !a.path().is_ident("cfg") {
                continue;
            }
            let Some(feature) = feature_of(&a.meta.to_token_stream().to_string()) else {
                continue;
            };
            let pins = pins_in(&node);
            if !pins.is_empty() {
                self.found.push((a.span(), feature, pins));
                return true;
            }
        }
        false
    }
}

impl<'ast> Visit<'ast> for Gated {
    fn visit_item(&mut self, i: &'ast syn::Item) {
        let attrs: &[syn::Attribute] = match i {
            syn::Item::Const(x) => &x.attrs,
            syn::Item::Static(x) => &x.attrs,
            syn::Item::Fn(x) => &x.attrs,
            syn::Item::Mod(x) => &x.attrs,
            syn::Item::Struct(x) => &x.attrs,
            syn::Item::Impl(x) => &x.attrs,
            syn::Item::Use(x) => &x.attrs,
            syn::Item::Type(x) => &x.attrs,
            syn::Item::Macro(x) => &x.attrs,
            _ => &[],
        };
        if !self.check(attrs, i) {
            visit::visit_item(self, i);
        }
    }

    fn visit_local(&mut self, l: &'ast syn::Local) {
        if !self.check(&l.attrs, l) {
            visit::visit_local(self, l);
        }
    }

    fn visit_arm(&mut self, a: &'ast syn::Arm) {
        if !self.check(&a.attrs, a) {
            visit::visit_arm(self, a);
        }
    }

    fn visit_field_value(&mut self, f: &'ast syn::FieldValue) {
        if !self.check(&f.attrs, f) {
            visit::visit_field_value(self, f);
        }
    }

    fn visit_stmt(&mut self, s: &'ast syn::Stmt) {
        if let syn::Stmt::Expr(e, _) = s {
            let attrs: &[syn::Attribute] = match e {
                syn::Expr::Block(b) => &b.attrs,
                syn::Expr::Call(c) => &c.attrs,
                syn::Expr::MethodCall(m) => &m.attrs,
                syn::Expr::Assign(a) => &a.attrs,
                syn::Expr::Macro(m) => &m.attrs,
                _ => &[],
            };
            if self.check(attrs, e) {
                return;
            }
        }
        if let syn::Stmt::Macro(m) = s
            && self.check(&m.attrs, m)
        {
            return;
        }
        visit::visit_stmt(self, s);
    }

    fn visit_expr_if(&mut self, i: &'ast syn::ExprIf) {
        if let syn::Expr::Macro(m) = &*i.cond
            && m.mac.path.is_ident("cfg")
            && let Some(feature) = feature_of(&m.mac.tokens.to_string())
        {
            let pins = pins_in(&i.then_branch);
            if !pins.is_empty() {
                self.found.push((m.span(), feature, pins));
                return;
            }
        }
        visit::visit_expr_if(self, i);
    }
}
