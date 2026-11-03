//! `IRQ-6` Nested disable / enable of interrupts by hand

use super::util::{text, walk_exprs, walk_macros};
use crate::check::{Check, CheckMeta, Need, Registered};
use crate::finding::{Confidence, Finding};
use crate::model::Firmware;
use crate::model::context::bodies;
use syn::spanned::Spanned;
use syn::{Expr, Lit};

pub struct Irq6;

static META: CheckMeta = CheckMeta {
    id: "IRQ-6",
    needs: &[Need::Sources],
};

inventory::submit!(Registered(&Irq6));

impl Check for Irq6 {
    fn meta(&self) -> &'static CheckMeta {
        &META
    }

    /// `cortex_m::interrupt::disable()` and `::enable()` (or `cpsid` / `cpsie`, as a call or inline asm) both
    /// called in the same function: a hand-rolled critical section, which an outer one or a nested enable can
    /// reopen early; `critical_section::with` tracks nesting by itself
    fn run(&self, fw: &Firmware) -> Vec<Finding> {
        let mut out = Vec::new();
        for fb in bodies(&fw.sources) {
            let mut disable = None;
            let mut enable = None;
            walk_exprs(fb.block, &mut |e| {
                let Expr::Call(c) = e else { return };
                let Expr::Path(p) = &*c.func else { return };
                let path = text(&p.path);
                let Some(last) = p.path.segments.last().map(|s| s.ident.to_string()) else {
                    return;
                };
                let interrupt_like = path.to_ascii_lowercase().contains("interrupt");
                let slot = match last.as_str() {
                    "disable" if interrupt_like => &mut disable,
                    "cpsid" => &mut disable,
                    "enable" if interrupt_like => &mut enable,
                    "cpsie" => &mut enable,
                    _ => return,
                };
                if slot.is_none() {
                    *slot = Some((fb.loc(p.path.span()), path));
                }
            });
            walk_macros(fb.block, &mut |m| {
                if m.path.segments.last().map(|s| s.ident.to_string()).as_deref() != Some("asm") {
                    return;
                }
                let Ok(args) =
                    m.parse_body_with(syn::punctuated::Punctuated::<Expr, syn::Token![,]>::parse_terminated)
                else {
                    return;
                };
                let Some(Expr::Lit(l)) = args.first() else { return };
                let Lit::Str(s) = &l.lit else { return };
                let asm = s.value();
                if disable.is_none() && asm.contains("cpsid") {
                    disable = Some((fb.loc(m.span()), "asm!(\"cpsid ..\")".to_string()));
                }
                if enable.is_none() && asm.contains("cpsie") {
                    enable = Some((fb.loc(m.span()), "asm!(\"cpsie ..\")".to_string()));
                }
            });
            let (Some((dloc, dtext)), Some((eloc, etext))) = (disable, enable) else {
                continue;
            };
            out.push(
                Finding::new(
                    "IRQ-6",
                    fb.path.clone(),
                    format!(
                        "{dtext} and {etext} both disable and re-enable interrupts by hand in the same function: \
                         nested under an outer critical section, the inner enable turns interrupts back on too \
                         early"
                    ),
                )
                .span(dloc.span().label(dtext))
                .span(eloc.span().label(etext))
                .fix("use `critical_section::with(|cs| ..)`, which nests correctly")
                .confidence(Confidence::Estimate),
            );
        }
        out
    }
}
