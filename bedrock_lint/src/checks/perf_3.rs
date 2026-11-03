//! `PERF-3` RAM function (`#[link_section = ".data.*"]`) without `#[inline(never)]`

use crate::check::{Check, CheckMeta, Need, Registered};
use crate::finding::{Confidence, Finding, Span};
use crate::model::Firmware;
use crate::model::elf::SymKind;
use syn::visit::{self, Visit};

pub struct Perf3;

static META: CheckMeta = CheckMeta {
    id: "PERF-3",
    needs: &[Need::Sources, Need::Elf],
};

inventory::submit!(Registered(&Perf3));

/// A function the source puts in a RAM section
struct RamFn {
    name: String,
    section: String,
    inline_never: bool,
    line: u32,
    column: u32,
}

fn ram_fns(ast: &syn::File) -> Vec<RamFn> {
    struct V(Vec<RamFn>);
    fn look(v: &mut V, attrs: &[syn::Attribute], name: &syn::Ident, span: proc_macro2::Span) {
        let mut section = None;
        let mut inline_never = false;
        for a in attrs {
            let tokens = quote::ToTokens::to_token_stream(&a.meta).to_string();
            if let Some(rest) = tokens.split("link_section").nth(1)
                && let Some(value) = rest.split('"').nth(1)
            {
                section = Some(value.to_string());
            }
            if a.path().is_ident("inline") && tokens.contains("never") {
                inline_never = true;
            }
        }
        if let Some(section) = section
            .filter(|s| s.starts_with(".data") || s.starts_with(".ram") || s.starts_with(".sram"))
        {
            let s = span.start();
            v.0.push(RamFn {
                name: name.to_string(),
                section,
                inline_never,
                line: s.line as u32,
                column: s.column as u32 + 1,
            });
        }
    }
    impl<'ast> Visit<'ast> for V {
        fn visit_item_fn(&mut self, f: &'ast syn::ItemFn) {
            look(self, &f.attrs, &f.sig.ident, f.sig.fn_token.span);
            visit::visit_item_fn(self, f);
        }
        fn visit_impl_item_fn(&mut self, f: &'ast syn::ImplItemFn) {
            look(self, &f.attrs, &f.sig.ident, f.sig.fn_token.span);
            visit::visit_impl_item_fn(self, f);
        }
    }
    let mut v = V(Vec::new());
    v.visit_file(ast);
    v.0
}

impl Check for Perf3 {
    fn meta(&self) -> &'static CheckMeta {
        &META
    }

    /// The ELF part: a function the source places in RAM that has no function symbol of its name in a writable
    /// section of the ELF was inlined back into its callers in flash. Certain without `#[inline(never)]`; with it
    /// an estimate (the function may be unused and dropped).
    fn run(&self, fw: &Firmware) -> Vec<Finding> {
        let Some(elf) = &fw.elf else {
            return Vec::new();
        };
        let mut out = Vec::new();
        for src in &fw.sources {
            for f in ram_fns(&src.ast) {
                let suffix = format!("::{}", f.name);
                let in_ram = elf.symbols.iter().any(|s| {
                    s.kind == SymKind::Func
                        && (s.name == f.name || s.name.ends_with(&suffix))
                        && s.section
                            .as_ref()
                            .and_then(|n| elf.section(n))
                            .is_some_and(|x| {
                                x.write || fw.region_at(x.addr).is_some_and(|r| r.is_ram())
                            })
                });
                if in_ram {
                    continue;
                }
                let mut finding = Finding::new(
                    "PERF-3",
                    format!("{}:{}", src.rel, f.name),
                    format!(
                        "`{}` is placed in {} but the ELF has no copy of it in RAM: it was inlined into its callers and runs from flash",
                        f.name, f.section
                    ),
                )
                .span(Span::at(&src.rel, f.line, f.column))
                .fix("add `#[inline(never)]` next to the link_section, so LTO keeps the function and its RAM placement");
                if f.inline_never {
                    finding = finding.confidence(Confidence::Estimate);
                }
                out.push(finding);
            }
        }
        out
    }
}
