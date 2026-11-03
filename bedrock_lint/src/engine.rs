//! A run: pick the checks, run those whose data is there, report the others as not run with the reason, apply the
//! project's severities, the source waivers and the baseline, order the findings.

use crate::baseline::Baseline;
use crate::catalogue::{catalogue, order_key};
use crate::check::{Check, registered};
use crate::config::Config;
use crate::finding::{Finding, NotRun};
use crate::model::Firmware;
use crate::report::{Report, Skipped};
use crate::waiver;
use std::fmt;

/// What to run
#[derive(Clone, Debug, Default)]
pub struct RunOptions {
    /// Only these check ids or categories (`EXE-3`, `STK`); empty for all
    pub only: Vec<String>,
    pub config: Config,
    /// Mark findings new or unchanged against this
    pub baseline: Option<Baseline>,
}

/// Why a run could not start
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RunError {
    /// Neither a check id nor a category of the catalogue
    UnknownCheck(String),
}

impl fmt::Display for RunError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RunError::UnknownCheck(c) => write!(f, "no check or category {c} in the catalogue"),
        }
    }
}

impl std::error::Error for RunError {}

/// Runs every registered check on the firmware
pub fn run(fw: &Firmware, opts: &RunOptions) -> Result<Report, RunError> {
    run_with(&registered(), fw, opts)
}

/// Runs `checks` on the firmware
pub fn run_with(
    checks: &[&'static dyn Check],
    fw: &Firmware,
    opts: &RunOptions,
) -> Result<Report, RunError> {
    let cat = catalogue();
    for o in &opts.only {
        if cat.entry(o).is_none() && cat.category(o).is_none() {
            return Err(RunError::UnknownCheck(o.clone()));
        }
    }
    let selected: Vec<&'static dyn Check> = checks
        .iter()
        .copied()
        .filter(|c| {
            let id = c.meta().id;
            let category = id.split('-').next().unwrap_or(id);
            opts.only.is_empty()
                || opts
                    .only
                    .iter()
                    .any(|o| o.eq_ignore_ascii_case(id) || o.eq_ignore_ascii_case(category))
        })
        .collect();

    let mut findings = Vec::new();
    let mut not_run = Vec::new();
    let mut skipped = Vec::new();
    let mut ran = Vec::new();
    for c in &selected {
        let meta = c.meta();
        if let Some((id, reason)) = opts
            .config
            .skip
            .iter()
            .find(|(id, _)| id.eq_ignore_ascii_case(meta.id))
        {
            skipped.push(Skipped {
                check: id.clone(),
                reason: reason.clone(),
            });
            continue;
        }
        if let Some((need, reason)) = meta
            .needs
            .iter()
            .find_map(|n| n.missing(fw).map(|r| (n, r)))
        {
            not_run.push(NotRun {
                check: meta.id.to_string(),
                need: need.name().to_string(),
                reason,
            });
            continue;
        }
        ran.push(meta.id.to_string());
        findings.extend(c.run_with(fw, &opts.config));
    }

    for f in &mut findings {
        if let Some(s) = opts
            .config
            .severity
            .iter()
            .find(|(id, _)| id.eq_ignore_ascii_case(&f.check))
            .map(|(_, s)| *s)
        {
            f.severity = s;
        }
    }

    let (waivers, problems) = waiver::collect(fw);
    let mut used = vec![false; waivers.len()];
    for f in &mut findings {
        if let Some(w) = waiver::find(&waivers, f) {
            f.waived = Some(w.reason.clone());
            if let Some(i) = waivers.iter().position(|x| std::ptr::eq(x, w)) {
                used[i] = true;
            }
        }
    }
    // a waiver of a check that did not run this time is not unused
    let unused = waivers
        .iter()
        .zip(&used)
        .filter(|(w, used)| !**used && w.checks.iter().all(|c| ran.iter().any(|r| r == c)))
        .map(|(w, _)| w.clone())
        .collect();

    if let Some(b) = &opts.baseline {
        let mut live: Vec<Finding> = findings
            .iter()
            .filter(|f| f.waived.is_none())
            .cloned()
            .collect();
        b.mark(&mut live);
        let mut marks = live.into_iter();
        for f in findings.iter_mut().filter(|f| f.waived.is_none()) {
            f.baseline = marks.next().and_then(|m| m.baseline);
        }
    }

    findings.sort_by(|a, b| {
        (a.severity, order_key(&a.check), a.primary(), &a.item).cmp(&(
            b.severity,
            order_key(&b.check),
            b.primary(),
            &b.item,
        ))
    });
    not_run.sort_by_key(|n| order_key(&n.check));

    Ok(Report {
        tool: crate::VERSION.to_string(),
        firmware: fw.name.clone(),
        dir: fw.dir.display().to_string(),
        chip: fw.chip().map(|c| c.name.clone()),
        findings,
        ran,
        not_run,
        skipped,
        notes: fw.notes.clone(),
        problems,
        unused_waivers: unused,
        compared_with_baseline: opts.baseline.is_some(),
    })
}
