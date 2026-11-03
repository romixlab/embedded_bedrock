//! Helpers the checks share: walking a body's expressions (macro bodies included), priorities as numbers, the
//! names of logging macros.

use quote::ToTokens;
use syn::punctuated::Punctuated;
use syn::visit::{self, Visit};
use syn::{Expr, Token};

/// Every expression of a block, macro bodies that parse as expressions included, nested `fn` items left out
pub fn walk_exprs(block: &syn::Block, f: &mut dyn FnMut(&Expr)) {
    struct W<'f> {
        f: &'f mut dyn FnMut(&Expr),
    }
    impl<'ast> Visit<'ast> for W<'_> {
        fn visit_expr(&mut self, e: &'ast Expr) {
            (self.f)(e);
            visit::visit_expr(self, e);
        }
        fn visit_macro(&mut self, m: &'ast syn::Macro) {
            if let Ok(args) = m.parse_body_with(Punctuated::<Expr, Token![,]>::parse_terminated) {
                for a in &args {
                    self.visit_expr(a);
                }
            }
        }
        fn visit_item_fn(&mut self, _: &'ast syn::ItemFn) {}
    }
    W { f }.visit_block(block);
}

/// Every macro call of a block, in expression and statement position, also inside other macros' bodies; nested
/// `fn` items left out
pub fn walk_macros(block: &syn::Block, f: &mut dyn FnMut(&syn::Macro)) {
    struct W<'f> {
        f: &'f mut dyn FnMut(&syn::Macro),
    }
    impl<'ast> Visit<'ast> for W<'_> {
        fn visit_macro(&mut self, m: &'ast syn::Macro) {
            (self.f)(m);
            if let Ok(args) = m.parse_body_with(Punctuated::<Expr, Token![,]>::parse_terminated) {
                for a in &args {
                    self.visit_expr(a);
                }
            }
        }
        fn visit_item_fn(&mut self, _: &'ast syn::ItemFn) {}
    }
    W { f }.visit_block(block);
}

/// The last segment of a macro's path (`defmt::info` → `info`)
pub fn macro_name(m: &syn::Macro) -> String {
    m.path
        .segments
        .last()
        .map(|s| s.ident.to_string())
        .unwrap_or_default()
}

/// defmt, log and print macros
pub fn is_log_macro(name: &str) -> bool {
    matches!(
        name,
        "trace" | "debug" | "info" | "warn" | "error" | "println" | "print" | "rprintln"
    )
}

/// Tokens as text with spaces folded around `::` and `.`
pub fn text(t: &impl ToTokens) -> String {
    t.to_token_stream()
        .to_string()
        .replace(" :: ", "::")
        .replace(" . ", ".")
}

/// A priority as written as a level, 0 the highest: `Priority::P3` → 3, `0` → 0. Raw NVIC values other than 0
/// depend on the chip's priority bits and are not told.
pub fn priority_level(s: &str) -> Option<u32> {
    let s = s.trim();
    if let Some(p) = s.rsplit("::").next().and_then(|l| l.strip_prefix('P')) {
        return p.parse().ok();
    }
    (s == "0").then_some(0)
}

/// Whether an expression's tokens name any of these identifiers
pub fn names_any(t: &impl ToTokens, idents: &[&str]) -> bool {
    fn go(ts: proc_macro2::TokenStream, idents: &[&str]) -> bool {
        ts.into_iter().any(|t| match t {
            proc_macro2::TokenTree::Ident(i) => idents.iter().any(|n| i == n),
            proc_macro2::TokenTree::Group(g) => go(g.stream(), idents),
            _ => false,
        })
    }
    go(t.to_token_stream(), idents)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn walks_into_macros() {
        let b: syn::Block =
            syn::parse_str("{ unwrap!(i2c.blocking_write(1, &[0])); let x = y(); }").unwrap();
        let mut methods = Vec::new();
        walk_exprs(&b, &mut |e| {
            if let Expr::MethodCall(m) = e {
                methods.push(m.method.to_string());
            }
        });
        assert_eq!(methods, ["blocking_write"]);
        assert_eq!(priority_level("Priority::P2"), Some(2));
        assert_eq!(priority_level("1 << 4"), None);
    }
}
