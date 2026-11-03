//! `EXE-2` Blocking call in a task on an interrupt executor or in an interrupt handler

use super::util::{macro_name, text, walk_exprs, walk_macros};
use crate::check::{Check, CheckMeta, Need, Registered};
use crate::finding::{Confidence, Finding};
use crate::model::context::{Context, bodies};
use crate::model::{Firmware, Loc};
use syn::Expr;
use syn::spanned::Spanned;

pub struct Exe2;

static META: CheckMeta = CheckMeta {
    id: "EXE-2",
    needs: &[Need::Sources],
};

inventory::submit!(Registered(&Exe2));

/// A call that blocks, by name: `block_for`, `block_on`, `blocking_*`
fn blocking_name(name: &str) -> bool {
    matches!(name, "block_for" | "block_on") || name.starts_with("blocking_")
}

/// A `while` whose body does nothing but spin
fn busy_wait(w: &syn::ExprWhile) -> bool {
    w.body.stmts.iter().all(|s| {
        let t = text(s);
        t.contains("spin_loop") || t.contains("nop")
    })
}

impl Check for Exe2 {
    fn meta(&self) -> &'static CheckMeta {
        &META
    }

    /// In every function that runs in interrupt context: calls named `block_for`, `block_on`, `blocking_*`,
    /// `nb::block!`, and `while` loops whose body is empty or only spins. Certain in the task or handler itself, an
    /// estimate in what it calls (calls are resolved by name)
    fn run(&self, fw: &Firmware) -> Vec<Finding> {
        let cx = Context::new(&fw.facts);
        let bodies = bodies(&fw.sources);
        let mut out = Vec::new();
        for (&i, reach) in &cx.interrupt {
            let f = cx.function(i);
            let Some(body) = bodies.iter().find(|b| b.path == f.path) else {
                continue;
            };
            let mut sites: Vec<(String, Loc)> = Vec::new();
            walk_exprs(body.block, &mut |e| match e {
                Expr::Call(c) => {
                    if let Expr::Path(p) = &*c.func
                        && let Some(last) = p.path.segments.last()
                        && blocking_name(&last.ident.to_string())
                    {
                        sites.push((text(&p.path), body.loc(p.span())));
                    }
                }
                Expr::MethodCall(m) if blocking_name(&m.method.to_string()) => {
                    sites.push((format!(".{}()", m.method), body.loc(m.method.span())));
                }
                Expr::While(w) if busy_wait(w) => {
                    sites.push((
                        format!("busy-wait `while {}`", text(&*w.cond)),
                        body.loc(w.while_token.span),
                    ));
                }
                _ => {}
            });
            walk_macros(body.block, &mut |m| {
                if macro_name(m) == "block" {
                    sites.push((format!("{}!", text(&m.path)), body.loc(m.path.span())));
                }
            });
            sites.sort_by(|a, b| a.1.cmp(&b.1));
            sites.dedup_by(|a, b| a.1 == b.1);
            let root = cx.function(reach.root);
            let context = if fw.facts.handlers.iter().any(|h| h.path == root.path) {
                format!("interrupt handler {}", root.name)
            } else {
                format!("task {} on an interrupt executor", root.name)
            };
            for (what, loc) in sites {
                let mut finding = Finding::new(
                    "EXE-2",
                    format!("{}:{}", f.path, what),
                    format!(
                        "{what} blocks in {context}{}: everything at its priority and below waits",
                        if reach.direct() {
                            String::new()
                        } else {
                            format!(" (through {})", cx.chain_text(reach))
                        }
                    ),
                )
                .span(loc.span().label(what.clone()))
                .fix("await instead (the async driver call, `Timer::after`), or move the blocking work to a thread-mode task")
                .confidence(if reach.direct() {
                    Confidence::Certain
                } else {
                    Confidence::Estimate
                });
                if !reach.direct() {
                    finding = finding.span(root.loc.span().label(context.clone()));
                }
                out.push(finding);
            }
        }
        out
    }
}
