//! Walking function bodies, for the checks that look at more than the [`Facts`](super::Facts): every function of the
//! source with its body ([`functions`]), a walk over a body that also enters macro calls ([`walk`]: `unwrap!(..)`,
//! `info!(..)`, `cnt!(..)`, `join!(..)` are parsed as Rust), the names an expression mentions, and what the firmware
//! runs on (core, chip family) when `bedrock_fw.json` does not say.
//!
//! Code under `#[cfg(test)]` and `#[test]` functions is left out: it never runs on the target.

use super::Firmware;
use super::source::{Loc, SourceFile, path_text, tokens, type_name};
use std::collections::BTreeSet;
use syn::punctuated::Punctuated;
use syn::spanned::Spanned;
use syn::visit::{self, Visit};
use syn::{Expr, Token};

/// A function or method with its body
#[derive(Clone)]
pub struct FnItem<'a> {
    pub file: &'a SourceFile,
    pub attrs: &'a [syn::Attribute],
    pub sig: &'a syn::Signature,
    pub block: &'a syn::Block,
    /// The `impl` type it is a method of (`ServerState`)
    pub self_ty: Option<&'a syn::Type>,
    /// The trait of that `impl` (`WireWeaverAsyncApiBackend`)
    pub trait_: Option<&'a syn::Path>,
    /// The module path inside the crate (`ww::i2c`)
    module: std::rc::Rc<[String]>,
}

impl<'a> FnItem<'a> {
    pub fn name(&self) -> String {
        self.sig.ident.to_string()
    }

    /// `crate::module::name`, `crate::module::Type::method`, as [`Function::path`](super::Function::path)
    pub fn path(&self) -> String {
        let mut p = vec![self.file.krate.clone()];
        p.extend(self.module.iter().cloned());
        if let Some(t) = self.self_ty {
            p.push(type_name(t));
        }
        p.push(self.name());
        p.join("::")
    }

    /// The `impl` type's last segment
    pub fn self_ty_name(&self) -> Option<String> {
        self.self_ty.map(type_name)
    }

    pub fn loc(&self) -> Loc {
        loc(self.file, self.sig.ident.span())
    }

    /// Whether an attribute's last path segment is `name` (`exception`, `interrupt`, `entry`)
    pub fn has_attr(&self, name: &str) -> bool {
        self.attrs
            .iter()
            .any(|a| a.path().segments.last().is_some_and(|s| s.ident == name))
    }

    /// The typed parameters (not `self`): the names bound by the pattern and the type as written
    pub fn params(&self) -> Vec<(Vec<String>, String)> {
        self.sig
            .inputs
            .iter()
            .filter_map(|a| match a {
                syn::FnArg::Typed(t) => Some((pat_idents(&t.pat), tokens(&t.ty))),
                syn::FnArg::Receiver(_) => None,
            })
            .collect()
    }

    pub fn has_self(&self) -> bool {
        self.sig.receiver().is_some()
    }
}

/// A place in a source file
pub fn loc(file: &SourceFile, span: proc_macro2::Span) -> Loc {
    let s = span.start();
    Loc {
        file: file.rel.clone(),
        line: s.line as u32,
        column: s.column as u32 + 1,
    }
}

/// Every function and method of the source with a body, test code left out
pub fn functions(fw: &Firmware) -> Vec<FnItem<'_>> {
    let mut out = Vec::new();
    for f in &fw.sources {
        let module: Vec<String> = f
            .module
            .split("::")
            .filter(|s| !s.is_empty())
            .map(str::to_string)
            .collect();
        items(f, &f.ast.items, module, &mut out);
    }
    out
}

fn items<'a>(
    file: &'a SourceFile,
    list: &'a [syn::Item],
    module: Vec<String>,
    out: &mut Vec<FnItem<'a>>,
) {
    let module: std::rc::Rc<[String]> = module.into();
    for item in list {
        match item {
            syn::Item::Fn(f) if !is_test(&f.attrs) => out.push(FnItem {
                file,
                attrs: &f.attrs,
                sig: &f.sig,
                block: &f.block,
                self_ty: None,
                trait_: None,
                module: module.clone(),
            }),
            syn::Item::Impl(i) if !is_test(&i.attrs) => {
                for it in &i.items {
                    if let syn::ImplItem::Fn(f) = it
                        && !is_test(&f.attrs)
                    {
                        out.push(FnItem {
                            file,
                            attrs: &f.attrs,
                            sig: &f.sig,
                            block: &f.block,
                            self_ty: Some(&i.self_ty),
                            trait_: i.trait_.as_ref().map(|(_, p, _)| p),
                            module: module.clone(),
                        });
                    }
                }
            }
            syn::Item::Mod(m) if !is_test(&m.attrs) => {
                if let Some((_, inner)) = &m.content {
                    let mut sub = module.to_vec();
                    sub.push(m.ident.to_string());
                    items(file, inner, sub, out);
                }
            }
            _ => {}
        }
    }
}

/// `#[test]`, `#[cfg(test)]`
fn is_test(attrs: &[syn::Attribute]) -> bool {
    attrs.iter().any(|a| {
        let p = path_text(a.path());
        p == "test"
            || p.ends_with("::test")
            || (p == "cfg" && tokens(&a.meta).replace(' ', "") == "cfg(test)")
    })
}

/// The last segment of a macro's path (`defmt::unwrap` → `unwrap`)
pub fn macro_name(m: &syn::Macro) -> String {
    m.path
        .segments
        .last()
        .map(|s| s.ident.to_string())
        .unwrap_or_default()
}

/// A macro body read as Rust: a comma-separated list of expressions (`unwrap!(x)`, `info!("{}", x)`), else a list
/// of statements (`select! { .. }` bodies that are blocks); empty when it is neither
pub fn macro_exprs(m: &syn::Macro) -> Vec<Expr> {
    if let Ok(args) = m.parse_body_with(Punctuated::<Expr, Token![,]>::parse_terminated) {
        return args.into_iter().collect();
    }
    if let Ok(stmts) = m.parse_body_with(syn::Block::parse_within) {
        return vec![Expr::Block(syn::ExprBlock {
            attrs: Vec::new(),
            label: None,
            block: syn::Block {
                brace_token: Default::default(),
                stmts,
            },
        })];
    }
    Vec::new()
}

/// What a walk over a body reports; every method has an empty default
pub trait Inspect {
    /// An expression, before its parts
    fn enter(&mut self, _e: &Expr) {}
    /// An expression, after its parts
    fn leave(&mut self, _e: &Expr) {}
    /// A macro call (expression, statement or item), before its body
    fn mac(&mut self, _name: &str, _m: &syn::Macro) {}
    /// A `let`, before its initialiser
    fn local(&mut self, _l: &syn::Local) {}
    /// A nested function item: walked unless this says no
    fn nested_fn(&mut self, _f: &syn::ItemFn) -> bool {
        false
    }
}

/// Walks a body, entering macro calls
pub fn walk(block: &syn::Block, i: &mut dyn Inspect) {
    Deep(i).visit_block(block);
}

/// Walks an expression, entering macro calls
pub fn walk_expr(e: &Expr, i: &mut dyn Inspect) {
    Deep(i).visit_expr(e);
}

struct Deep<'i>(&'i mut dyn Inspect);

impl Deep<'_> {
    fn macro_call(&mut self, m: &syn::Macro) {
        let name = macro_name(m);
        self.0.mac(&name, m);
        // a fresh walker: the parsed body lives shorter than the tree being walked
        for e in macro_exprs(m) {
            Deep(&mut *self.0).visit_expr(&e);
        }
    }
}

impl<'ast> Visit<'ast> for Deep<'_> {
    fn visit_expr(&mut self, e: &'ast Expr) {
        self.0.enter(e);
        match e {
            Expr::Macro(m) => self.macro_call(&m.mac),
            _ => visit::visit_expr(self, e),
        }
        self.0.leave(e);
    }

    fn visit_stmt_macro(&mut self, s: &'ast syn::StmtMacro) {
        self.macro_call(&s.mac);
    }

    fn visit_item_macro(&mut self, m: &'ast syn::ItemMacro) {
        self.macro_call(&m.mac);
    }

    fn visit_local(&mut self, l: &'ast syn::Local) {
        self.0.local(l);
        visit::visit_local(self, l);
    }

    fn visit_item_fn(&mut self, f: &'ast syn::ItemFn) {
        if self.0.nested_fn(f) {
            visit::visit_item_fn(self, f);
        }
    }

    // items nested in a body other than functions (impls, mods) are left alone: [`functions`] lists their methods
    fn visit_item_impl(&mut self, _: &'ast syn::ItemImpl) {}
    fn visit_item_mod(&mut self, _: &'ast syn::ItemMod) {}
}

/// Collects into a closure: every expression and macro of a body
struct Each<F: FnMut(Node<'_>)>(F);

/// What [`each`] hands out
pub enum Node<'n> {
    Expr(&'n Expr),
    Macro(&'n str, &'n syn::Macro),
}

impl<F: FnMut(Node<'_>)> Inspect for Each<F> {
    fn enter(&mut self, e: &Expr) {
        (self.0)(Node::Expr(e))
    }
    fn mac(&mut self, name: &str, m: &syn::Macro) {
        (self.0)(Node::Macro(name, m))
    }
}

/// Every expression and macro call of a body, macro bodies included
pub fn each(block: &syn::Block, f: impl FnMut(Node<'_>)) {
    walk(block, &mut Each(f));
}

/// Every expression and macro call of an expression, macro bodies included
pub fn each_in(e: &Expr, f: impl FnMut(Node<'_>)) {
    walk_expr(e, &mut Each(f));
}

/// The single-segment names an expression mentions (`len`, `self`), macro bodies included
pub fn idents(e: &Expr) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    each_in(e, |n| {
        if let Node::Expr(Expr::Path(p)) = n
            && p.path.segments.len() == 1
        {
            out.insert(p.path.segments[0].ident.to_string());
        }
    });
    out
}

/// The names a pattern binds (`Some((a, b))` → a, b)
pub fn pat_idents(p: &syn::Pat) -> Vec<String> {
    struct V(Vec<String>);
    impl<'ast> Visit<'ast> for V {
        fn visit_pat_ident(&mut self, i: &'ast syn::PatIdent) {
            self.0.push(i.ident.to_string());
            visit::visit_pat_ident(self, i);
        }
    }
    let mut v = V(Vec::new());
    v.visit_pat(p);
    v.0
}

/// The counting macros of the `cnt` crate
pub const COUNT_MACROS: &[&str] = &["cnt", "cnt_if", "bkp_cnt", "bkp_cnt_if"];

/// Whether an expression counts an event: a `cnt!` family macro or a `.count(..)` on a `cnt::Counters`
pub fn counts(e: &Expr) -> bool {
    let mut found = false;
    each_in(e, |n| match n {
        Node::Macro(name, _) if COUNT_MACROS.contains(&name) => found = true,
        Node::Expr(Expr::MethodCall(m)) if m.method == "count" && !m.args.is_empty() => {
            found = true
        }
        _ => {}
    });
    found
}

/// The name of a call expression: a path call as written (`cortex_m::peripheral::SCB::sys_reset`), a method call as
/// `.name`
pub fn call_name(e: &Expr) -> Option<String> {
    match e {
        Expr::Call(c) => match &*c.func {
            Expr::Path(p) => Some(path_text(&p.path)),
            _ => None,
        },
        Expr::MethodCall(m) => Some(format!(".{}", m.method)),
        _ => None,
    }
}

/// An expression's tokens as text (`interrupt::USB` with `::` folded)
pub fn text(e: &impl quote::ToTokens) -> String {
    tokens(e)
}

/// The span of an expression's head: the method name of a method call, the path of a call, else the whole
pub fn head_span(e: &Expr) -> proc_macro2::Span {
    match e {
        Expr::MethodCall(m) => m.method.span(),
        Expr::Call(c) => c.func.span(),
        _ => e.span(),
    }
}

/// The Cortex-M core the firmware is built for, from the target triple: `M0+` (thumbv6m), `M3` (thumbv7m), `M4/M7`
/// (thumbv7em), `M33` (thumbv8m.main), `M23` (thumbv8m.base); `None` for other targets or without one
pub fn core(fw: &Firmware) -> Option<&'static str> {
    let (_, t) = fw.build_target()?;
    Some(match t.split('-').next()? {
        "thumbv6m" => "M0+",
        "thumbv7m" => "M3",
        "thumbv7em" => "M4/M7",
        "thumbv8m.main" => "M33",
        "thumbv8m.base" => "M23",
        _ => return None,
    })
}

/// The chip family (`stm32g0`, `stm32h7`, `rp2040`, `rp235x`, `nrf52`): `bedrock_fw.json`'s chip, else the chip
/// feature of the HAL dependency (`embassy-stm32` `stm32g0b1re`, `embassy-rp` `rp235xa`); `rp2040` for `embassy-rp`
/// without a chip feature
pub fn family(fw: &Firmware) -> Option<String> {
    if let Some(c) = fw.chip() {
        return Some(c.family.clone());
    }
    for d in fw.dependencies() {
        let pkg = d.package().to_string();
        let feats: Vec<String> = d.features().into_iter().map(|(f, _, _)| f).collect();
        match pkg.as_str() {
            "embassy-stm32" | "stm32-metapac" => {
                if let Some(f) = feats.iter().find(|f| f.starts_with("stm32")) {
                    return Some(super::Chip::new(f).family);
                }
            }
            "embassy-rp" | "rp-pac" => {
                return Some(if feats.iter().any(|f| f.starts_with("rp235")) {
                    "rp235x".into()
                } else {
                    "rp2040".into()
                });
            }
            "rp2040-hal" => return Some("rp2040".into()),
            "rp235x-hal" => return Some("rp235x".into()),
            p if p.starts_with("stm32") && p.ends_with("xx-hal") => {
                return Some(p.trim_end_matches("xx-hal").to_string());
            }
            p if p.starts_with("embassy-nrf") || p.starts_with("nrf52") => {
                return Some("nrf52".into());
            }
            _ => {}
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn file(src: &str) -> SourceFile {
        SourceFile {
            rel: "src/main.rs".into(),
            krate: "fw".into(),
            module: String::new(),
            text: src.into(),
            ast: syn::parse_file(src).unwrap(),
        }
    }

    #[test]
    fn walks_into_macros_and_skips_tests() {
        let f = file(
            r#"
fn a(x: u32) { defmt::info!("{}", unwrap!(x.checked_add(1))); cnt!(a_seen); }
impl S { fn m(&self) {} }
mod inner { fn b() {} }
#[cfg(test)]
mod tests { fn t() {} }
#[test]
fn t2() {}
"#,
        );
        let mut out = Vec::new();
        items(&f, &f.ast.items, Vec::new(), &mut out);
        let paths: Vec<String> = out.iter().map(|f| f.path()).collect();
        assert_eq!(paths, ["fw::a", "fw::S::m", "fw::inner::b"]);
        let mut macros = Vec::new();
        let mut methods = Vec::new();
        each(out[0].block, |n| match n {
            Node::Macro(name, _) => macros.push(name.to_string()),
            Node::Expr(Expr::MethodCall(m)) => methods.push(m.method.to_string()),
            _ => {}
        });
        assert_eq!(macros, ["info", "unwrap", "cnt"]);
        assert_eq!(methods, ["checked_add"]);
        assert_eq!(
            out[0].params(),
            [(vec!["x".to_string()], "u32".to_string())]
        );
        let body = Expr::Block(syn::ExprBlock {
            attrs: Vec::new(),
            label: None,
            block: out[0].block.clone(),
        });
        assert!(counts(&body));
        assert!(idents(&body).contains("x"));
    }
}
