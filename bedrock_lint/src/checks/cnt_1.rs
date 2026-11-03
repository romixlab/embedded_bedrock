//! `CNT-1` Error branch without a `cnt!` counter

use crate::check::{Check, CheckMeta, Need, Registered};
use crate::finding::{Confidence, Finding};
use crate::model::Firmware;
use crate::model::scan::{self, FnItem, Inspect, Node};
use std::collections::{BTreeMap, BTreeSet};
use syn::Expr;
use syn::spanned::Spanned;

pub struct Cnt1;

static META: CheckMeta = CheckMeta {
    id: "CNT-1",
    needs: &[Need::Sources],
};

inventory::submit!(Registered(&Cnt1));

impl Check for Cnt1 {
    fn meta(&self) -> &'static CheckMeta {
        &META
    }

    /// Error branches: `Err(..) =>` arms, `if let Err(..) =`, `if x.is_err()`, `let Ok(..) = .. else`, and the
    /// timer arm of a `select` (`Either::Second(_)` on a match whose scrutinee names `Timer` or a timeout). A branch
    /// is fine when it counts (`cnt!`, `.count(..)`, or calls a function of the source, unique by name, that counts),
    /// hands the error on (`Err` in its body, `?`), or panics (crash capture records it). `WouldBlock` and `Empty`
    /// are not errors. A branch that swallows the error (empty, or only logs) is an estimate; one that does more is a
    /// candidate: the counter may live in the code it calls.
    fn run(&self, fw: &Firmware) -> Vec<Finding> {
        let fns = scan::functions(fw);
        let counting = counting_fns(&fns);
        let mut out = Vec::new();
        for f in &fns {
            let mut w = Walk {
                counting: &counting,
                found: Vec::new(),
            };
            scan::walk(f.block, &mut w);
            let path = f.path();
            let mut n: BTreeMap<String, usize> = BTreeMap::new();
            for (span, what, body) in w.found {
                let swallowed = swallows(&body);
                let key = format!("{path}:{what}");
                let i = n.entry(key.clone()).or_default();
                *i += 1;
                out.push(
                    Finding::new(
                        "CNT-1",
                        format!("{key}#{i}"),
                        if swallowed {
                            format!("`{what}` is swallowed in `{path}` without a counter: nobody sees it happen")
                        } else {
                            format!("`{what}` is handled in `{path}` without a counter")
                        },
                    )
                    .span(scan::loc(f.file, span).span().label("error branch"))
                    .fix("count it: `cnt!(what_happened)` in the branch (cnt crate), so it shows in `cnt read`")
                    .confidence(if swallowed {
                        Confidence::Estimate
                    } else {
                        Confidence::Candidate
                    }),
                );
            }
        }
        out
    }
}

/// Functions whose body counts, by name (a name defined twice counts only when both count)
fn counting_fns(fns: &[FnItem]) -> BTreeSet<String> {
    let mut by_name: BTreeMap<String, bool> = BTreeMap::new();
    for f in fns {
        let mut c = false;
        scan::each(f.block, |n| match n {
            Node::Macro(name, _) if scan::COUNT_MACROS.contains(&name) => c = true,
            Node::Expr(Expr::MethodCall(m)) if m.method == "count" && !m.args.is_empty() => {
                c = true
            }
            _ => {}
        });
        by_name.entry(f.name()).and_modify(|v| *v &= c).or_insert(c);
    }
    by_name
        .into_iter()
        .filter(|(_, c)| *c)
        .map(|(n, _)| n)
        .collect()
}

/// Empty, or nothing but logging and `continue` / `break` / `return` without a value
fn swallows(body: &Expr) -> bool {
    fn stmt_ok(s: &syn::Stmt) -> bool {
        match s {
            syn::Stmt::Macro(m) => is_log(&scan::macro_name(&m.mac)),
            syn::Stmt::Expr(e, _) => expr_ok(e),
            _ => false,
        }
    }
    fn expr_ok(e: &Expr) -> bool {
        match e {
            Expr::Block(b) => b.block.stmts.iter().all(stmt_ok),
            Expr::Macro(m) => is_log(&scan::macro_name(&m.mac)),
            Expr::Continue(_) => true,
            Expr::Break(b) => b.expr.is_none(),
            Expr::Return(r) => r.expr.is_none(),
            Expr::Tuple(t) => t.elems.is_empty(),
            _ => false,
        }
    }
    expr_ok(body)
}

fn is_log(name: &str) -> bool {
    matches!(
        name,
        "trace" | "debug" | "info" | "warn" | "error" | "println" | "print"
    )
}

struct Walk<'a> {
    counting: &'a BTreeSet<String>,
    /// (where, what, the branch)
    found: Vec<(proc_macro2::Span, String, Expr)>,
}

impl Walk<'_> {
    /// Whether a branch needs no counter of its own
    fn fine(&self, body: &Expr) -> bool {
        if scan::counts(body) {
            return true;
        }
        let mut fine = false;
        scan::each_in(body, |n| match n {
            Node::Expr(Expr::Try(_)) => fine = true,
            Node::Expr(Expr::Path(p))
                if p.path.segments.last().is_some_and(|s| s.ident == "Err") =>
            {
                fine = true
            }
            Node::Expr(e @ (Expr::Call(_) | Expr::MethodCall(_))) => {
                if let Some(name) = scan::call_name(e) {
                    let last = name
                        .trim_start_matches('.')
                        .rsplit("::")
                        .next()
                        .unwrap_or("");
                    if self.counting.contains(last) {
                        fine = true;
                    }
                }
            }
            Node::Macro("panic" | "unreachable" | "todo" | "unimplemented" | "assert", _) => {
                fine = true
            }
            _ => {}
        });
        fine
    }

    fn check(&mut self, span: proc_macro2::Span, what: String, body: Expr) {
        if !self.fine(&body) {
            self.found.push((span, what, body));
        }
    }
}

fn block_expr(b: &syn::Block) -> Expr {
    Expr::Block(syn::ExprBlock {
        attrs: Vec::new(),
        label: None,
        block: b.clone(),
    })
}

/// `Err(..)` as a pattern, not one of the expected "nothing there" errors
fn err_pat(p: &syn::Pat) -> bool {
    let t = scan::text(p);
    let is_err = match p {
        syn::Pat::TupleStruct(ts) => ts.path.segments.last().is_some_and(|s| s.ident == "Err"),
        syn::Pat::Or(o) => o.cases.iter().any(err_pat),
        _ => false,
    };
    is_err && !t.contains("WouldBlock") && !t.contains("Empty")
}

impl Inspect for Walk<'_> {
    fn enter(&mut self, e: &Expr) {
        match e {
            Expr::Match(m) => {
                let scrut = scan::text(&m.expr).to_lowercase();
                for arm in &m.arms {
                    if err_pat(&arm.pat) {
                        self.check(arm.pat.span(), scan::text(&arm.pat), (*arm.body).clone());
                    } else if (scrut.contains("timer") || scrut.contains("timeout"))
                        && scan::text(&arm.pat).contains("Second")
                    {
                        self.check(arm.pat.span(), "timeout".into(), (*arm.body).clone());
                    }
                }
            }
            Expr::If(i) => match &*i.cond {
                Expr::Let(l) if err_pat(&l.pat) => {
                    self.check(l.pat.span(), scan::text(&l.pat), block_expr(&i.then_branch));
                }
                Expr::MethodCall(m) if m.method == "is_err" => {
                    self.check(
                        m.method.span(),
                        scan::text(&i.cond),
                        block_expr(&i.then_branch),
                    );
                }
                _ => {}
            },
            _ => {}
        }
    }

    fn local(&mut self, l: &syn::Local) {
        if let Some(init) = &l.init
            && let Some((_, els)) = &init.diverge
            && let syn::Pat::TupleStruct(ts) = &l.pat
            && ts.path.segments.last().is_some_and(|s| s.ident == "Ok")
        {
            self.check(
                l.let_token.span,
                format!("let {} else", scan::text(&l.pat)),
                (**els).clone(),
            );
        }
    }
}
