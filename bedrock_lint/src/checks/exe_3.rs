//! `EXE-3` Interrupt executor started before its IRQ priority is set

use crate::check::{Check, CheckMeta, Need, Registered};
use crate::finding::{Confidence, Finding};
use crate::model::Firmware;

pub struct Exe3;

static META: CheckMeta = CheckMeta {
    id: "EXE-3",
    needs: &[Need::Sources],
};

inventory::submit!(Registered(&Exe3));

impl Check for Exe3 {
    fn meta(&self) -> &'static CheckMeta {
        &META
    }

    /// For each `start(VECTOR)` of an interrupt executor: a `set_priority` of that vector earlier in the same
    /// function is fine; one later in it is certain; none anywhere is certain; one only in another function is fine
    /// when that function is called earlier in the starting function, else an estimate (the order is not known)
    fn run(&self, fw: &Firmware) -> Vec<Finding> {
        let facts = &fw.facts;
        let mut out = Vec::new();
        for s in &facts.executor_starts {
            let item = format!("{}:{}", s.in_fn, s.executor);
            let fix = format!(
                "call `interrupt::{}.set_priority(Priority::P..)` before `{}.start(..)`",
                if s.vector.is_empty() {
                    "<vector>"
                } else {
                    &s.vector
                },
                s.executor
            );
            if s.vector.is_empty() {
                out.push(
                    Finding::new(
                        "EXE-3",
                        item,
                        format!(
                            "interrupt executor {} is started on a vector that is not a plain path: check that its priority is set before",
                            s.executor
                        ),
                    )
                    .span(s.loc.span())
                    .fix(fix)
                    .confidence(Confidence::Candidate),
                );
                continue;
            }
            let sets: Vec<_> = facts.priorities_of(&s.vector).collect();
            let here: Vec<_> = sets.iter().filter(|p| p.in_fn == s.in_fn).collect();
            if here.iter().any(|p| p.loc < s.loc) {
                continue;
            }
            if let Some(late) = here.first() {
                out.push(
                    Finding::new(
                        "EXE-3",
                        item,
                        format!(
                            "interrupt executor {} is started before the priority of {} is set: start() unmasks it at the highest priority, above the drivers that wake its tasks",
                            s.executor, s.vector
                        ),
                    )
                    .span(s.loc.span().label(format!("{}.start({})", s.executor, s.vector)))
                    .span(late.loc.span().label(format!("priority set after it ({})", late.priority)))
                    .fix(fix),
                );
                continue;
            }
            if sets.is_empty() {
                out.push(
                    Finding::new(
                        "EXE-3",
                        item,
                        format!(
                            "the priority of {} is never set: interrupt executor {} runs at the highest priority, above the drivers that wake its tasks",
                            s.vector, s.executor
                        ),
                    )
                    .span(s.loc.span().label(format!("{}.start({})", s.executor, s.vector)))
                    .fix(fix),
                );
                continue;
            }
            // Set in other functions: fine when one of them is called before the start
            let called_before = facts.function(&s.in_fn).is_some_and(|f| {
                f.calls.iter().filter(|c| c.loc < s.loc).any(|c| {
                    let callee = c.name.trim_start_matches('.');
                    let callee = callee.rsplit("::").next().unwrap_or(callee);
                    sets.iter()
                        .any(|p| p.in_fn.rsplit("::").next() == Some(callee))
                })
            });
            if called_before {
                continue;
            }
            let mut f = Finding::new(
                "EXE-3",
                item,
                format!(
                    "the priority of {} is set only in another function ({}): check that it runs before {}.start()",
                    s.vector,
                    sets.iter().map(|p| p.in_fn.as_str()).collect::<Vec<_>>().join(", "),
                    s.executor
                ),
            )
            .span(s.loc.span().label(format!("{}.start({})", s.executor, s.vector)))
            .fix(fix)
            .confidence(Confidence::Estimate);
            for p in &sets {
                f = f.span(p.loc.span().label(format!("set_priority({})", p.priority)));
            }
            out.push(f);
        }
        out
    }
}
