//! `EXE-4` Interrupt executor priority not below the interrupts that wake its tasks

use super::util::{priority_level, walk_exprs};
use crate::check::{Check, CheckMeta, Need, Registered};
use crate::finding::{Confidence, Finding};
use crate::model::context::{Context, Exec, bodies};
use crate::model::{Firmware, Loc};
use std::collections::BTreeSet;

pub struct Exe4;

static META: CheckMeta = CheckMeta {
    id: "EXE-4",
    needs: &[Need::Sources],
};

inventory::submit!(Registered(&Exe4));

/// An interrupt that may wake an executor's tasks, with its level (0 the highest)
struct Waker {
    name: String,
    level: u32,
    /// Where the level is set, `None` for the reset default
    set_at: Option<Loc>,
    at: Loc,
    /// Whether the executor's tasks are seen to use its peripheral
    used: bool,
}

impl Check for Exe4 {
    fn meta(&self) -> &'static CheckMeta {
        &META
    }

    /// For each interrupt executor whose priority is set as a level (`Priority::Pn`): every `bind_interrupts!`
    /// vector at the same level or below it (its own `set_priority`, else 0, the reset value embassy drivers leave),
    /// and embassy-time's alarm interrupt (`time_interrupt_priority`, P0 by default). An estimate when the vector's
    /// peripheral is handed to or named by the executor's tasks, else a candidate.
    fn run(&self, fw: &Firmware) -> Vec<Finding> {
        let facts = &fw.facts;
        let cx = Context::new(facts);
        let bodies = bodies(&fw.sources);
        let uses_time = fw
            .dependencies()
            .iter()
            .any(|d| d.package() == "embassy-time");
        // `config.time_interrupt_priority = Priority::P2`
        let mut time_level: Option<(u32, Loc)> = None;
        for b in &bodies {
            walk_exprs(b.block, &mut |e| {
                if let syn::Expr::Assign(a) = e
                    && let syn::Expr::Field(f) = &*a.left
                    && matches!(&f.member, syn::Member::Named(n) if n == "time_interrupt_priority")
                    && let Some(l) = priority_level(&super::util::text(&*a.right))
                {
                    time_level = Some((l, b.loc(syn::spanned::Spanned::span(&*a.left))));
                }
            });
        }
        let mut out = Vec::new();
        for s in &facts.executor_starts {
            let sets: Vec<_> = facts.priorities_of(&s.vector).collect();
            let Some(set) = sets
                .iter()
                .rfind(|p| p.in_fn == s.in_fn && p.loc < s.loc)
                .or(sets.first())
            else {
                continue; // EXE-3
            };
            let Some(level) = priority_level(&set.priority) else {
                continue;
            };
            // what the executor's tasks are handed and name
            let mut used: BTreeSet<String> = BTreeSet::new();
            let mut roots = Vec::new();
            for (task, execs) in &cx.task_execs {
                for (e, spawn) in execs {
                    if !matches!(e, Exec::Interrupt { executor, .. } if *executor == s.executor) {
                        continue;
                    }
                    if let Some(caller) = facts.function(&spawn.in_fn) {
                        for c in caller
                            .calls
                            .iter()
                            .filter(|c| c.name.rsplit("::").next() == Some(task))
                        {
                            used.extend(c.args.iter().cloned());
                        }
                    }
                    if let Some(i) = facts.functions.iter().position(|f| f.name == *task) {
                        roots.push(i);
                    }
                }
            }
            let reach = cx.reach(&roots);
            for p in &facts.peripherals {
                if reach.keys().any(|&i| facts.functions[i].path == p.in_fn) {
                    used.insert(p.name.clone());
                }
            }
            let mut wakers = Vec::new();
            let mut seen = BTreeSet::new();
            for b in &facts.bindings {
                if b.vector == s.vector || !seen.insert(b.vector.clone()) {
                    continue;
                }
                let own = facts
                    .priorities_of(&b.vector)
                    .find_map(|p| priority_level(&p.priority).map(|l| (l, p.loc.clone())));
                let (l, set_at) = match own {
                    Some((l, at)) => (l, Some(at)),
                    None => (0, None),
                };
                let names = super::exe_5::vector_peripherals(&b.vector);
                let in_handlers: Vec<String> = b
                    .handlers
                    .split(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
                    .filter(|w| crate::model::is_upper_name(w))
                    .map(str::to_string)
                    .collect();
                wakers.push(Waker {
                    name: b.vector.clone(),
                    level: l,
                    set_at,
                    at: b.loc.clone(),
                    used: names.iter().chain(&in_handlers).any(|n| used.contains(n)),
                });
            }
            if uses_time {
                let (l, at) = match &time_level {
                    Some((l, at)) => (*l, Some(at.clone())),
                    None => (0, None),
                };
                wakers.push(Waker {
                    name: "embassy-time's alarm".into(),
                    level: l,
                    at: at.clone().unwrap_or_else(|| s.loc.clone()),
                    set_at: at,
                    used: true,
                });
            }
            let late: Vec<&Waker> = wakers.iter().filter(|w| w.level >= level).collect();
            if late.is_empty() {
                continue;
            }
            let confidence = if late.iter().any(|w| w.used) {
                Confidence::Estimate
            } else {
                Confidence::Candidate
            };
            let list = late
                .iter()
                .map(|w| {
                    format!(
                        "{} at P{}{}",
                        w.name,
                        w.level,
                        if w.set_at.is_none() { " (default)" } else { "" }
                    )
                })
                .collect::<Vec<_>>()
                .join(", ");
            let mut f = Finding::new(
                "EXE-4",
                format!("{}:{}", s.in_fn, s.executor),
                format!(
                    "interrupt executor {} runs at P{level}, not below {list}: while its tasks run, the interrupts that wake them wait",
                    s.executor
                ),
            )
            .span(s.loc.span().label(format!("{}.start({}) at P{level}", s.executor, s.vector)))
            .fix(format!(
                "give {} a lower priority (a higher P number) than every interrupt that wakes its tasks, or raise theirs (`interrupt::X.set_priority(..)`, `config.time_interrupt_priority`)",
                s.vector
            ))
            .confidence(confidence);
            for w in &late {
                f = f.span(
                    w.set_at
                        .as_ref()
                        .unwrap_or(&w.at)
                        .span()
                        .label(format!("{} at P{}", w.name, w.level)),
                );
            }
            out.push(f);
        }
        out
    }
}
