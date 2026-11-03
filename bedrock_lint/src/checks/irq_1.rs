//! `IRQ-1` Interrupt flags cleared wholesale

use super::util::{names_any, text, walk_exprs};
use crate::check::{Check, CheckMeta, Need, Registered};
use crate::finding::Finding;
use crate::model::Firmware;
use crate::model::context::bodies;
use syn::spanned::Spanned;
use syn::{Expr, ExprLit, ExprUnary, Lit, UnOp};

pub struct Irq1;

static META: CheckMeta = CheckMeta {
    id: "IRQ-1",
    needs: &[Need::Sources],
};

inventory::submit!(Registered(&Irq1));

/// Receiver-chain names that read as an interrupt flag / clear register
const REGISTER_NAMES: &[&str] = &["ir", "icr", "ifcr", "isr", "sr", "clr"];

impl Check for Irq1 {
    fn meta(&self) -> &'static CheckMeta {
        &META
    }

    /// `<reg>.write_value(ALL_ONES)` where the receiver chain names a flag/clear register, or a register's read
    /// value written straight back to it (`x.write_value(x.read())`, `write_volatile(p, read_volatile(p))`)
    fn run(&self, fw: &Firmware) -> Vec<Finding> {
        let mut out = Vec::new();
        for fb in bodies(&fw.sources) {
            walk_exprs(fb.block, &mut |e| match e {
                Expr::MethodCall(m) if m.method == "write_value" && m.args.len() == 1 => {
                    let arg = &m.args[0];
                    let loc = fb.loc(m.method.span());
                    if is_all_ones(arg) && names_any(&m.receiver, REGISTER_NAMES) {
                        out.push(
                            Finding::new(
                                "IRQ-1",
                                format!("{}:{}", fb.path, loc.line),
                                format!(
                                    "all the flags of {} are cleared at once: an event set between the read that \
                                     decided this and the clear is lost; clear only the flags handled",
                                    text(&m.receiver)
                                ),
                            )
                            .span(loc.span().label(format!("{}.write_value({})", text(&m.receiver), text(arg)))),
                        );
                    } else if let Expr::MethodCall(r) = strip(arg)
                        && r.method == "read"
                        && r.args.is_empty()
                        && text(&r.receiver) == text(&m.receiver)
                    {
                        out.push(
                            Finding::new(
                                "IRQ-1",
                                format!("{}:{}", fb.path, loc.line),
                                format!(
                                    "{} is written back its own read value: a flag set between the read and the \
                                     write is lost; clear only the flags handled",
                                    text(&m.receiver)
                                ),
                            )
                            .span(loc.span().label(format!("{0}.write_value({0}.read())", text(&m.receiver)))),
                        );
                    }
                }
                Expr::Call(c) => {
                    let Expr::Path(p) = strip(&c.func) else {
                        return;
                    };
                    if !p
                        .path
                        .segments
                        .last()
                        .is_some_and(|s| s.ident == "write_volatile")
                        || c.args.len() != 2
                    {
                        return;
                    }
                    let ptr = &c.args[0];
                    let val = &c.args[1];
                    let Expr::Call(rc) = strip(val) else { return };
                    let Expr::Path(rp) = strip(&rc.func) else {
                        return;
                    };
                    if !rp
                        .path
                        .segments
                        .last()
                        .is_some_and(|s| s.ident == "read_volatile")
                        || rc.args.len() != 1
                        || text(ptr) != text(&rc.args[0])
                    {
                        return;
                    }
                    let loc = fb.loc(p.path.span());
                    out.push(
                        Finding::new(
                            "IRQ-1",
                            format!("{}:{}", fb.path, loc.line),
                            format!(
                                "{} is written back its own read_volatile value: a flag set in between is lost",
                                text(ptr)
                            ),
                        )
                        .span(loc.span().label(format!("write_volatile({0}, read_volatile({0}))", text(ptr)))),
                    );
                }
                _ => {}
            });
        }
        out
    }
}

/// Through `&`, `*` and parens, to the expression that matters
fn strip(e: &Expr) -> &Expr {
    match e {
        Expr::Paren(p) => strip(&p.expr),
        Expr::Reference(r) => strip(&r.expr),
        Expr::Unary(u) if matches!(u.op, UnOp::Deref(_)) => strip(&u.expr),
        _ => e,
    }
}

/// `0xFFFF_FFFF`, `!0` (any width), `u32::MAX` (or u8 / u16 / u64)
fn is_all_ones(e: &Expr) -> bool {
    match strip(e) {
        Expr::Lit(ExprLit {
            lit: Lit::Int(i), ..
        }) => matches!(
            i.base10_parse::<u64>(),
            Ok(0xFF | 0xFFFF | 0xFFFF_FFFF | u64::MAX)
        ),
        Expr::Unary(ExprUnary {
            op: UnOp::Not(_),
            expr,
            ..
        }) => matches!(
            strip(expr),
            Expr::Lit(ExprLit { lit: Lit::Int(i), .. }) if i.base10_digits() == "0"
        ),
        Expr::Path(p) => {
            let segs: Vec<String> = p
                .path
                .segments
                .iter()
                .map(|s| s.ident.to_string())
                .collect();
            matches!(segs.as_slice(), [ty, max] if max == "MAX" && matches!(ty.as_str(), "u8" | "u16" | "u32" | "u64"))
        }
        _ => false,
    }
}
