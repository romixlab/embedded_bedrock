//! `PIN-1` Pin driver dropped

use super::pin;
use crate::check::{Check, CheckMeta, Need, Registered};
use crate::finding::{Confidence, Finding};
use crate::model::Firmware;
use crate::model::scan::{self, Node};
use syn::Expr;

pub struct Pin1;

static META: CheckMeta = CheckMeta {
    id: "PIN-1",
    needs: &[Need::Sources],
};

inventory::submit!(Registered(&Pin1));

const FIX: &str = "keep the driver alive for as long as the pin must hold its state: move it into a task or a static, or `core::mem::forget` it on purpose";

impl Check for Pin1 {
    fn meta(&self) -> &'static CheckMeta {
        &META
    }

    /// A GPIO driver (`Output`, `Input`, `Flex`, `OutputOpenDrain`, `ExtiInput`) built and dropped at once: `let _ =
    /// Output::new(..)` or `Output::new(..);` is certain. One bound to a name in the entry point that is never handed
    /// on (an argument, a struct field, a spawn) while the entry point returns (no endless `loop` and no `.await` as
    /// its last statement) is an estimate: embassy's `main` returns and drops it, and the pin goes back to its reset
    /// state.
    fn run(&self, fw: &Firmware) -> Vec<Finding> {
        let entries: Vec<&str> = fw.facts.entries.iter().map(|e| e.path.as_str()).collect();
        let mut out = Vec::new();
        for f in scan::functions(fw) {
            let path = f.path();
            for (i, stmt) in f.block.stmts.iter().enumerate() {
                match stmt {
                    syn::Stmt::Local(l) => {
                        let Some(init) = &l.init else { continue };
                        let Some(ty) = pin::driver_new(&init.expr) else {
                            continue;
                        };
                        let span = scan::loc(f.file, scan::head_span(&init.expr)).span();
                        if matches!(&l.pat, syn::Pat::Wild(_)) {
                            out.push(
                                Finding::new("PIN-1", format!("{path}:let _:{ty}"), format!("`let _ = {ty}::new(..)` drops the driver at once: embassy disconnects the pin again"))
                                    .span(span.label("dropped here"))
                                    .fix(FIX),
                            );
                            continue;
                        }
                        if !entries.contains(&path.as_str()) {
                            continue;
                        }
                        let names = scan::pat_idents(&l.pat);
                        let rest = &f.block.stmts[i + 1..];
                        if handed_on(rest, &names) || keeps_running(&f.block.stmts) {
                            continue;
                        }
                        out.push(
                            Finding::new(
                                "PIN-1",
                                format!("{path}:{}", names.join(",")),
                                format!("`{}` ({ty}) lives in `{path}`, which returns: the driver is dropped there and the pin goes back to its reset state", names.join(", ")),
                            )
                            .span(span.label("dropped when this function returns"))
                            .fix(FIX)
                            .confidence(Confidence::Estimate),
                        );
                    }
                    syn::Stmt::Expr(e, Some(_)) => {
                        if let Some(ty) = pin::driver_new(e) {
                            out.push(
                                Finding::new("PIN-1", format!("{path}:temporary:{ty}"), format!("`{ty}::new(..);` is a temporary: the driver is dropped at the end of the statement and the pin disconnected"))
                                    .span(scan::loc(f.file, scan::head_span(e)).span().label("dropped here"))
                                    .fix(FIX),
                            );
                        }
                    }
                    _ => {}
                }
            }
        }
        out
    }
}

/// A name passed to a call, a macro, a struct literal or a closure later on
fn handed_on(rest: &[syn::Stmt], names: &[String]) -> bool {
    let block = syn::Block {
        brace_token: Default::default(),
        stmts: rest.to_vec(),
    };
    let mut moved = false;
    scan::each(&block, |n| {
        let args: Vec<&Expr> = match n {
            Node::Expr(Expr::Call(c)) => c.args.iter().collect(),
            Node::Expr(Expr::MethodCall(m)) => m.args.iter().collect(),
            Node::Expr(Expr::Struct(s)) => s.fields.iter().map(|f| &f.expr).collect(),
            Node::Expr(Expr::Closure(c)) => vec![&*c.body],
            Node::Expr(Expr::Return(r)) => r.expr.iter().map(|e| &**e).collect(),
            Node::Expr(Expr::Tuple(t)) => t.elems.iter().collect(),
            Node::Expr(Expr::Array(a)) => a.elems.iter().collect(),
            _ => Vec::new(),
        };
        if args
            .iter()
            .any(|a| scan::idents(a).iter().any(|i| names.contains(i)))
        {
            moved = true;
        }
    });
    moved
}

/// The function never returns: its last statement is an endless `loop` or an `.await`
fn keeps_running(stmts: &[syn::Stmt]) -> bool {
    match stmts.last() {
        Some(syn::Stmt::Expr(e, _)) => match e {
            Expr::Loop(_) | Expr::Await(_) => true,
            Expr::Macro(m) => scan::macro_name(&m.mac) == "join",
            _ => false,
        },
        _ => false,
    }
}
