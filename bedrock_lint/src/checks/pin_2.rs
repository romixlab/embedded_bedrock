//! `PIN-2` Board not put in its safe state first thing after the HAL init

use super::pin;
use crate::check::{Check, CheckMeta, Need, Registered};
use crate::finding::{Confidence, Finding};
use crate::model::Firmware;
use crate::model::scan::{self, FnItem, Node};
use std::collections::BTreeMap;
use syn::Expr;

pub struct Pin2;

static META: CheckMeta = CheckMeta {
    id: "PIN-2",
    needs: &[Need::Sources],
};

inventory::submit!(Registered(&Pin2));

impl Check for Pin2 {
    fn meta(&self) -> &'static CheckMeta {
        &META
    }

    /// The source part: in the entry point, after the HAL init (`..::init(..)`), the statements in order. Pin setup
    /// is an output driver built there (`Output::new`, `OutputOpenDrain::new`, `Flex::new`) or a call to a function
    /// of the source that builds one. Something slow is an `.await`, a spawn or a USB device being built. Something
    /// slow before the first pin setup is an estimate when the firmware drives outputs somewhere; outputs set up only
    /// outside the entry point are a candidate. Whether the order is the board's safe one stays a review question.
    fn run(&self, fw: &Firmware) -> Vec<Finding> {
        let fns = scan::functions(fw);
        let mut builds_outputs: BTreeMap<String, bool> = BTreeMap::new();
        for f in &fns {
            let b = builds_output(f);
            // by `Type::name` for methods, by name for functions (a name defined twice counts when both build)
            let key = match f.self_ty_name() {
                Some(t) => format!("{t}::{}", f.name()),
                None => f.name(),
            };
            builds_outputs
                .entry(key)
                .and_modify(|v| *v &= b)
                .or_insert(b);
        }
        let any_output = builds_outputs.values().any(|b| *b);
        if !any_output {
            return Vec::new();
        }
        let mut out = Vec::new();
        for e in &fw.facts.entries {
            let Some(f) = fns.iter().find(|f| f.path() == e.path) else {
                continue;
            };
            let mut after_init = false;
            let mut setup = false;
            for stmt in &f.block.stmts {
                let mut is_init = false;
                let mut is_setup = false;
                let mut slow: Option<(proc_macro2::Span, &'static str)> = None;
                each_stmt(stmt, |n| match n {
                    Node::Expr(e @ Expr::Call(_)) => {
                        let name = scan::call_name(e).unwrap_or_default();
                        let last = name.rsplit("::").next().unwrap_or("");
                        let last_two = name.rsplitn(3, "::").take(2).collect::<Vec<_>>();
                        let typed = (last_two.len() == 2)
                            .then(|| format!("{}::{}", last_two[1], last_two[0]));
                        if last == "init" && name.contains("::") {
                            is_init = true;
                        }
                        if pin::driver_new(e).is_some_and(|t| t != "Input" && t != "ExtiInput")
                            || builds_outputs.get(last).copied().unwrap_or(false)
                            || typed
                                .is_some_and(|t| builds_outputs.get(&t).copied().unwrap_or(false))
                        {
                            is_setup = true;
                        }
                        if slow.is_none()
                            && (name.ends_with("Builder::new") || name.contains("UsbDevice"))
                        {
                            slow = Some((scan::head_span(e), "USB device"));
                        }
                    }
                    Node::Expr(e @ Expr::MethodCall(m)) => {
                        let name = m.method.to_string();
                        if builds_outputs.get(&name).copied().unwrap_or(false) {
                            is_setup = true;
                        }
                        if slow.is_none() && (name == "spawn" || name == "must_spawn") {
                            slow = Some((scan::head_span(e), "spawn"));
                        }
                    }
                    Node::Expr(Expr::Await(a)) if slow.is_none() => {
                        slow = Some((a.await_token.span, ".await"));
                    }
                    _ => {}
                });
                if is_init {
                    after_init = true;
                    continue;
                }
                if !after_init {
                    continue;
                }
                if is_setup {
                    setup = true;
                    break;
                }
                if let Some((span, what)) = slow {
                    out.push(
                        Finding::new(
                            "PIN-2",
                            format!("{}:{what}", e.path),
                            format!("`{}` reaches a {what} before it sets up any output: until then the board's outputs, enables and switches float or sit at their reset state", e.path),
                        )
                        .span(scan::loc(f.file, span).span().label(format!("{what} before the safe state")))
                        .fix("drive every output enable, power switch and shift-register chain to its safe level right after the HAL init, before USB, timers or tasks")
                        .confidence(Confidence::Estimate),
                    );
                    setup = true;
                    break;
                }
            }
            if after_init && !setup {
                out.push(
                    Finding::new(
                        "PIN-2",
                        format!("{}:no-setup", e.path),
                        format!("`{}` sets up no outputs: they are set up later, in tasks, so the board is not in its safe state from the start", e.path),
                    )
                    .span(e.loc.span())
                    .fix("drive every output enable, power switch and shift-register chain to its safe level right after the HAL init")
                    .confidence(Confidence::Candidate),
                );
            }
        }
        out
    }
}

fn each_stmt(stmt: &syn::Stmt, f: impl FnMut(Node<'_>)) {
    let block = syn::Block {
        brace_token: Default::default(),
        stmts: vec![stmt.clone()],
    };
    scan::each(&block, f);
}

/// The function builds an output driver
fn builds_output(f: &FnItem) -> bool {
    let mut b = false;
    scan::each(f.block, |n| {
        if let Node::Expr(e @ Expr::Call(_)) = n
            && pin::driver_new(e).is_some_and(|t| t != "Input" && t != "ExtiInput")
        {
            b = true;
        }
    });
    b
}
