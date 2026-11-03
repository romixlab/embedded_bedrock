//! `EXE-8` Request server waits on a bus or a slow peripheral inside a handler

use super::util::{text, walk_exprs};
use crate::check::{Check, CheckMeta, Need, Registered};
use crate::finding::{Confidence, Finding};
use crate::model::Firmware;
use crate::model::context::bodies;
use syn::Expr;
use syn::visit::Visit;

pub struct Exe8;

static META: CheckMeta = CheckMeta {
    id: "EXE-8",
    needs: &[Need::Sources],
};

inventory::submit!(Registered(&Exe8));

/// Receivers that stand for a bus or a slow peripheral, by a word of their name
const BUSES: &[&str] = &[
    "i2c", "spi", "bus", "uart", "usart", "sd", "sdmmc", "flash", "eeprom", "can", "qspi", "ospi",
];

/// Types that implement a WireWeaver API backend (`impl WireWeaverAsyncApiBackend for ServerState`)
fn server_types(fw: &Firmware) -> Vec<String> {
    struct V(Vec<String>);
    impl<'a> Visit<'a> for V {
        fn visit_item_impl(&mut self, i: &'a syn::ItemImpl) {
            if let Some((_, t, _)) = &i.trait_
                && t.segments
                    .last()
                    .is_some_and(|s| s.ident.to_string().ends_with("ApiBackend"))
                && let syn::Type::Path(p) = &*i.self_ty
                && let Some(s) = p.path.segments.last()
            {
                self.0.push(s.ident.to_string());
            }
            syn::visit::visit_item_impl(self, i);
        }
    }
    let mut v = V(Vec::new());
    for f in &fw.sources {
        v.visit_file(&f.ast);
    }
    v.0
}

/// The bus word of a receiver that is a place (`self.i2c`, `sd_card`, `&mut self.spi1`): one of its last name's
/// `_`-separated words; a method chain (`stream().sd()`) is no bus
fn is_bus(recv: &Expr) -> Option<String> {
    let name = match recv {
        Expr::Path(p) => p.path.segments.last()?.ident.to_string(),
        Expr::Field(f) => match &f.member {
            syn::Member::Named(n) => n.to_string(),
            syn::Member::Unnamed(_) => return None,
        },
        Expr::Reference(r) => return is_bus(&r.expr),
        Expr::Paren(p) => return is_bus(&p.expr),
        _ => return None,
    }
    .to_lowercase();
    name.split('_')
        .find(|w| {
            BUSES.iter().any(|b| {
                w.strip_prefix(b)
                    .is_some_and(|n| n.chars().all(|c| c.is_ascii_digit()))
            })
        })
        .map(str::to_string)
}

impl Check for Exe8 {
    fn meta(&self) -> &'static CheckMeta {
        &META
    }

    /// In the async methods of a type that implements a WireWeaver API backend (the request handlers): an awaited
    /// method call on a receiver named after a bus (`self.i2c.write_read(..).await`): an estimate
    fn run(&self, fw: &Firmware) -> Vec<Finding> {
        let types = server_types(fw);
        if types.is_empty() {
            return Vec::new();
        }
        let mut out = Vec::new();
        for b in bodies(&fw.sources) {
            if b.sig.asyncness.is_none() {
                continue;
            }
            let Some(ty) = types
                .iter()
                .find(|t| b.path.ends_with(&format!("::{t}::{}", b.sig.ident)))
            else {
                continue;
            };
            walk_exprs(b.block, &mut |e| {
                let Expr::Await(a) = e else { return };
                let Expr::MethodCall(m) = &*a.base else {
                    return;
                };
                let Some(bus) = is_bus(&m.receiver) else {
                    return;
                };
                out.push(
                    Finding::new(
                        "EXE-8",
                        format!("{}:{}.{}", b.path, text(&*m.receiver), m.method),
                        format!(
                            "request handler {ty}::{} awaits `{}.{}()` ({bus}): the server answers nothing else until the transfer is done",
                            b.sig.ident,
                            text(&*m.receiver),
                            m.method
                        ),
                    )
                    .span(b.loc(m.method.span()).span().label(format!("{}.{}(..).await", text(&*m.receiver), m.method)))
                    .fix("hand the transfer to the task that owns the bus (a channel) and answer from the server loop when it is done")
                    .confidence(Confidence::Estimate),
                );
            });
        }
        out
    }
}
