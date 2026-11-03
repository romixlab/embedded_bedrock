//! `IRQ-2` `rc_w1` / W1C flags cleared with `modify()` instead of a write of the one bit

use crate::check::{Check, CheckMeta, Need, Registered};
use crate::finding::{Confidence, Finding};
use crate::model::Firmware;
use crate::model::context::{FnBody, bodies};
use quote::ToTokens;
use syn::visit::{self, Visit};
use syn::{Expr, UnOp};

pub struct Irq2;

static META: CheckMeta = CheckMeta {
    id: "IRQ-2",
    needs: &[Need::Sources],
};

inventory::submit!(Registered(&Irq2));

/// Receiver-chain names that read as a status / flag register, as the catalogue lists them
const REGISTER_NAMES: &[&str] = &["isr", "sr", "ir", "ifr"];

impl Check for Irq2 {
    fn meta(&self) -> &'static CheckMeta {
        &META
    }

    /// `<reg>.modify(|w| ..)` where the receiver chain names a status/flag register: a read-modify-write can clear
    /// a flag that was set between the read and the write, as embassy-stm32 0.6's G0 ADC ISR did to CCRDY. No SVD
    /// access types yet to tell a genuine R/W field from a `rc_w1` one, so this is always a candidate.
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

impl<'ast> Visit<'ast> for Finder<'_, '_> {
    fn visit_expr(&mut self, e: &'ast Expr) {
        if let Expr::MethodCall(m) = e
            && m.method == "modify"
            && m.args.len() == 1
            && matches!(&m.args[0], Expr::Closure(_))
            && names_a_flag_register(&m.receiver)
        {
            let loc = self.fb.loc(m.method.span());
            self.out.push(
                Finding::new(
                    "IRQ-2",
                    format!("{}:{}", self.fb.path, loc.line),
                    format!(
                        "{}.modify() reads, changes and writes back the whole register: a flag of it that was set \
                         between the read and the write is cleared unseen",
                        text(&m.receiver)
                    ),
                )
                .span(loc.span().label(format!("{}.modify(..)", text(&m.receiver))))
                .confidence(Confidence::Candidate),
            );
        }
        visit::visit_expr(self, e);
    }
}

fn strip(e: &Expr) -> &Expr {
    match e {
        Expr::Paren(p) => strip(&p.expr),
        Expr::Reference(r) => strip(&r.expr),
        Expr::Unary(u) if matches!(u.op, UnOp::Deref(_)) => strip(&u.expr),
        _ => e,
    }
}

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
