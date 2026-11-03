//! `EXE-9` Future polled with a no-op waker that can reach `embassy_time`

use crate::check::{Check, CheckMeta, Need, Registered};
use crate::finding::{Confidence, Finding};
use crate::model::Firmware;

pub struct Exe9;

static META: CheckMeta = CheckMeta {
    id: "EXE-9",
    needs: &[Need::Sources],
};

inventory::submit!(Registered(&Exe9));

/// Calls that poll with a waker no executor made
const NOOP: &[&str] = &[
    "noop_waker",
    "noop_waker_ref",
    "noop",
    "poll_once",
    "noop_context",
];

impl Check for Exe9 {
    fn meta(&self) -> &'static CheckMeta {
        &META
    }

    /// Calls of `noop_waker`, `Waker::noop`, `poll_once`, `noop_context`: an estimate when the firmware depends on
    /// embassy-time (the polled future may reach a timer), else a candidate
    fn run(&self, fw: &Firmware) -> Vec<Finding> {
        let time = fw
            .dependencies()
            .iter()
            .any(|d| d.package() == "embassy-time");
        let mut out = Vec::new();
        for f in &fw.facts.functions {
            for c in &f.calls {
                let last = c
                    .name
                    .trim_start_matches('.')
                    .rsplit("::")
                    .next()
                    .unwrap_or("");
                // `noop` alone is too common a name: only `Waker::noop`
                let hit = match last {
                    "noop" => c.name.ends_with("Waker::noop"),
                    _ => NOOP.contains(&last),
                };
                if !hit {
                    continue;
                }
                out.push(
                    Finding::new(
                        "EXE-9",
                        format!("{}:{}", f.path, c.name),
                        format!(
                            "{} polls with a waker no embassy executor made: a future that reaches embassy_time (a Timer, a timeout) panics \"Found waker not created by the Embassy executor\"",
                            c.name.trim_start_matches('.')
                        ),
                    )
                    .span(c.loc.span().label(c.name.trim_start_matches('.').to_string()))
                    .fix("poll it from a task (await it, or `select` with the work it races), or make sure nothing it polls touches embassy_time and waive this with that reason")
                    .confidence(if time {
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
