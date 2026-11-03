//! `EXE-1` A task with a hard answer deadline runs on the thread executor beside bulk work

use super::util::names_any;
use crate::check::{Check, CheckMeta, Need, Registered};
use crate::finding::{Confidence, Finding};
use crate::model::Firmware;
use crate::model::context::{Context, Exec, FnBody, bodies};

pub struct Exe1;

static META: CheckMeta = CheckMeta {
    id: "EXE-1",
    needs: &[Need::Sources],
};

inventory::submit!(Registered(&Exe1));

/// What marks work with an answer deadline: embassy's USB PD PHY (GoodCRC within the spec's turnaround; the
/// UCPD register block alone is no PHY), a DMA ring consumer that must keep up with the ring
const DEADLINE: &[&str] = &[
    "PdPhy",
    "split_pd_phy",
    "into_ring_buffered",
    "ReadableDmaRingBuffer",
    "RingBufferedUartRx",
];

/// What marks bulk work that holds the executor for long polls
const BULK: &[&str] = &["UsbDevice", "UsbServer", "embassy_usb", "usb_device"];

impl Check for Exe1 {
    fn meta(&self) -> &'static CheckMeta {
        &META
    }

    /// Thread-mode tasks whose code (with what it calls, functions that spawn left out) names a deadline marker
    /// (UCPD / a PD PHY, a DMA ring reader), when another thread-mode task is handed bulk work (a USB device or
    /// server as a parameter): a candidate, since whether the deadline is hard is the reader's call
    fn run(&self, fw: &Firmware) -> Vec<Finding> {
        let facts = &fw.facts;
        let cx = Context::new(facts);
        let bodies = bodies(&fw.sources);
        let body = |path: &str| -> Option<&FnBody<'_>> { bodies.iter().find(|b| b.path == path) };
        // the task and what it calls, leaving out functions that spawn tasks, the task too (setup, not the work)
        let deadline = |task: usize| -> Option<String> {
            cx.reach(&[task]).keys().find_map(|&i| {
                let f = cx.function(i);
                if facts.spawns.iter().any(|s| s.in_fn == f.path) {
                    return None;
                }
                let b = body(&f.path)?;
                DEADLINE
                    .iter()
                    .find(|m| names_any(b.sig, &[m]) || names_any(b.block, &[m]))
                    .map(|m| m.to_string())
            })
        };
        // bulk work is what the task is handed: its parameters' types
        let bulk_of = |task: usize| -> Option<String> {
            let b = body(&cx.function(task).path)?;
            BULK.iter()
                .find(|m| names_any(&b.sig.inputs, &[m]))
                .map(|m| m.to_string())
        };
        let thread_tasks: Vec<usize> = facts
            .tasks
            .iter()
            .filter_map(|t| {
                let i = facts.functions.iter().position(|f| f.path == t.path)?;
                let execs = cx.execs_of(cx.function(i));
                (execs.is_empty() || execs.contains(&&Exec::Thread)).then_some(i)
            })
            .collect();
        let bulk: Vec<(usize, String)> = thread_tasks
            .iter()
            .filter_map(|&i| bulk_of(i).map(|m| (i, m)))
            .collect();
        let mut out = Vec::new();
        for &i in &thread_tasks {
            let Some(marker) = deadline(i) else {
                continue;
            };
            let Some((b, bm)) = bulk.iter().find(|(b, _)| *b != i) else {
                continue;
            };
            let task = cx.function(i);
            let other = cx.function(*b);
            out.push(
                Finding::new(
                    "EXE-1",
                    task.path.clone(),
                    format!(
                        "task {} ({marker}) has an answer deadline and runs on the thread executor beside {} ({bm}): one long poll of that task and it misses it",
                        task.name, other.name
                    ),
                )
                .span(task.loc.span().label(format!("{} ({marker})", task.name)))
                .span(other.loc.span().label(format!("{} ({bm}) on the same executor", other.name)))
                .fix("move the deadline task to an InterruptExecutor on an unused vector, at a priority below its drivers' interrupts (EXE-4, EXE-5)")
                .confidence(Confidence::Candidate),
            );
        }
        out
    }
}
