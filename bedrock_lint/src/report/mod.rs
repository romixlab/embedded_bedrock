//! The result of a run and its forms: text for a terminal ([`text`]), JSON (the [`Report`] itself, serde) and SARIF
//! 2.1.0 for code-scanning viewers ([`sarif`]).

pub mod sarif;
pub mod text;

use crate::finding::{BaselineState, Confidence, Finding, NotRun, Severity};
use crate::model::Note;
use crate::waiver::{Problem, Waiver};
use serde::Serialize;

/// A check the project skips in `bedrock-lint.toml`
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Skipped {
    pub check: String,
    pub reason: String,
}

/// Everything a run found and did not do
#[derive(Clone, Debug, Serialize)]
pub struct Report {
    /// `bedrock-lint` version that made it
    pub tool: String,
    /// The package name
    pub firmware: String,
    pub dir: String,
    pub chip: Option<String>,
    /// Worst first, then catalogue order
    pub findings: Vec<Finding>,
    /// The checks that ran
    pub ran: Vec<String>,
    /// Registered checks whose data was missing
    pub not_run: Vec<NotRun>,
    pub skipped: Vec<Skipped>,
    /// What the loader could not read
    pub notes: Vec<Note>,
    /// Waivers written wrong; each fails the run
    pub problems: Vec<Problem>,
    /// Waivers of checks that ran and found nothing they cover
    pub unused_waivers: Vec<Waiver>,
    pub compared_with_baseline: bool,
}

/// From which severity findings fail the run
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FailOn {
    Severity(Severity),
    Never,
}

impl Report {
    /// The findings that fail the run: not waived, not candidates, not already in the baseline, at `fail_on` or
    /// worse
    pub fn failing(&self, fail_on: FailOn) -> Vec<&Finding> {
        let FailOn::Severity(level) = fail_on else {
            return Vec::new();
        };
        self.findings
            .iter()
            .filter(|f| {
                f.waived.is_none()
                    && f.confidence != Confidence::Candidate
                    && f.baseline != Some(BaselineState::Unchanged)
                    && f.severity <= level
            })
            .collect()
    }

    /// The run fails: failing findings or waivers written wrong
    pub fn fails(&self, fail_on: FailOn) -> bool {
        !self.failing(fail_on).is_empty() || !self.problems.is_empty()
    }

    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(self).expect("report serializes") + "\n"
    }

    /// Findings counted per severity, not waived
    pub fn counts(&self) -> [usize; 3] {
        let mut c = [0; 3];
        for f in self.findings.iter().filter(|f| f.waived.is_none()) {
            c[f.severity as usize] += 1;
        }
        c
    }
}
