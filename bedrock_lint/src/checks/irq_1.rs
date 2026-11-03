//! `IRQ-1` Interrupt flags cleared wholesale

use crate::check::{Check, CheckMeta, Need, Registered};
use crate::finding::Finding;
use crate::model::context::{FnBody, bodies};
use crate::model::Firmware;
use quote::ToTokens;
use syn::spanned::Spanned;
use syn::visit::{self, Visit};
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
            let mut v = Finder {
                fb: &fb,
                out: &mut out,
            };
            v.visit_block(fb.block);
        }
        out
    }
}

struct Finder<'a, 'o> {
    fb: &'a FnBody<'a>,
    out: &'o mut Vec<Finding>,
}

impl Finder<'_, '_> {
    fn found(&mut self, span: proc_macro2::Span, message: String, snippet: String) {
        let loc = self.fb.loc(span);
        self.out.push(
            Finding::new(
                "IRQ-1",
                format!("{}:{}", self.fb.path, loc.line),
                message,
            )
            .span(loc.span().label(snippet)),
        );
    }
}

impl<'ast> Visit<'ast> for Finder<'_, '_> {
    fn visit_expr(&mut self, e: &'ast Expr) {
        match e {
            Expr::MethodCall(m) if m.method == "write_value" && m.args.len() == 1 => {
                let arg = &m.args[0];
                if is_all_ones(arg) && names_a_flag_register(&m.receiver) {
                    self.found(
                        m.method.span(),
                        format!(
                            "all the flags of {} are cleared at once: an event set between the read that decided \
                             this and the clear is lost; clear only the flags handled",
                            text(&m.receiver)
                        ),
                        format!("{}.write_value({})", text(&m.receiver), text(arg)),
                    );
                } else if let Expr::MethodCall(r) = strip(arg)
                    && r.method == "read"
                    && r.args.is_empty()
                    && text(&r.receiver) == text(&m.receiver)
                {
                    self.found(
                        m.method.span(),
                        format!(
                            "{} is written back its own read value: a flag set between the read and the write is \
                             lost; clear only the flags handled",
                            text(&m.receiver)
                        ),
                        format!("{0}.write_value({0}.read())", text(&m.receiver)),
                    );
                }
            }
            Expr::Call(c) => {
                if let Expr::Path(p) = strip(&c.func)
                    && p.path.segments.last().is_some_and(|s| s.ident == "write_volatile")
                    && c.args.len() == 2
                {
                    let ptr = &c.args[0];
                    let val = &c.args[1];
                    if let Expr::Call(rc) = strip(val)
                        && let Expr::Path(rp) = strip(&rc.func)
                        && rp.path.segments.last().is_some_and(|s| s.ident == "read_volatile")
                        && rc.args.len() == 1
                        && text(ptr) == text(&rc.args[0])
                    {
                        self.found(
                            p.path.span(),
                            format!(
                                "{} is written back its own read_volatile value: a flag set in between is lost",
                                text(ptr)
                            ),
                            format!("write_volatile({0}, read_volatile({0}))", text(ptr)),
                        );
                    }
                }
            }
            _ => {}
        }
        visit::visit_expr(self, e);
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
            let segs: Vec<String> = p.path.segments.iter().map(|s| s.ident.to_string()).collect();
            matches!(segs.as_slice(), [ty, max] if max == "MAX" && matches!(ty.as_str(), "u8" | "u16" | "u32" | "u64"))
        }
        _ => false,
    }
}

/// Whether the receiver chain names a flag/clear register: the outermost method or field name, case-insensitive
fn names_a_flag_register(e: &Expr) -> bool {
    match strip(e) {
        Expr::MethodCall(m) => {
            let name = m.method.to_string().to_ascii_lowercase();
            REGISTER_NAMES.contains(&name.as_str()) || names_a_flag_register(&m.receiver)
        }
        Expr::Field(f) => {
            if let syn::Member::Named(n) = &f.member {
                let name = n.to_string().to_ascii_lowercase();
                if REGISTER_NAMES.contains(&name.as_str()) {
                    return true;
                }
            }
            names_a_flag_register(&f.base)
        }
        Expr::Path(p) => p.path.segments.last().is_some_and(|s| {
            REGISTER_NAMES.contains(&s.ident.to_string().to_ascii_lowercase().as_str())
        }),
        _ => false,
    }
}

fn text(e: &impl ToTokens) -> String {
    e.to_token_stream()
        .to_string()
        .replace(" :: ", "::")
        .replace(" . ", ".")
        .replace(" ( ", "(")
        .replace(" )", ")")
}
