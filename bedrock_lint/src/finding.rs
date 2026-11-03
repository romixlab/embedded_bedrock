//! [`Finding`]: one type for what every check reports, and [`NotRun`] for a check whose data is missing.

use serde::{Deserialize, Serialize};
use std::fmt;

/// How bad a finding is: the catalogue's three levels (🛑 ⚠️ 💡), worst first
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    /// The firmware misbehaves, hangs, bricks or damages hardware
    Error,
    /// A risk or a lost margin that depends on the board
    Warning,
    /// Size, speed, debuggability, conventions
    Suggestion,
}

impl Severity {
    pub const ALL: [Severity; 3] = [Severity::Error, Severity::Warning, Severity::Suggestion];

    pub fn name(self) -> &'static str {
        match self {
            Severity::Error => "error",
            Severity::Warning => "warning",
            Severity::Suggestion => "suggestion",
        }
    }

    pub fn from_name(name: &str) -> Option<Severity> {
        Severity::ALL.into_iter().find(|s| s.name() == name)
    }
}

impl fmt::Display for Severity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}

/// How sure a finding is
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Confidence {
    /// Every fact it rests on is in the firmware's files
    Certain,
    /// It rests on something the tool could not see whole (a call in another function, a value built at run time):
    /// the message says what was assumed
    Estimate,
    /// A heuristic: a place for a human or an agent to look at, never fails the run
    Candidate,
}

impl Confidence {
    pub fn name(self) -> &'static str {
        match self {
            Confidence::Certain => "certain",
            Confidence::Estimate => "estimate",
            Confidence::Candidate => "candidate",
        }
    }
}

/// A place in the firmware's files, relative to the firmware crate's directory (`src/main.rs`, `../Cargo.toml`)
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct Span {
    pub file: String,
    /// 1-based; 0 when the whole file is meant (a key that is missing)
    pub line: u32,
    /// 1-based column, 0 when unknown
    #[serde(default)]
    pub column: u32,
    /// The last line, for a span over several lines (0: one line)
    #[serde(default, skip_serializing_if = "is_zero")]
    pub end_line: u32,
    /// What is there (`InterruptExecutor EXECUTOR_HIGH started`)
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub label: String,
}

fn is_zero(n: &u32) -> bool {
    *n == 0
}

impl Span {
    pub fn new(file: impl Into<String>, line: u32) -> Span {
        Span {
            file: file.into(),
            line,
            column: 0,
            end_line: 0,
            label: String::new(),
        }
    }

    pub fn at(file: impl Into<String>, line: u32, column: u32) -> Span {
        Span {
            column,
            ..Span::new(file, line)
        }
    }

    pub fn label(mut self, label: impl Into<String>) -> Span {
        self.label = label.into();
        self
    }
}

impl fmt::Display for Span {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match (self.line, self.column) {
            (0, _) => write!(f, "{}", self.file),
            (l, 0) => write!(f, "{}:{l}", self.file),
            (l, c) => write!(f, "{}:{l}:{c}", self.file),
        }
    }
}

/// One thing a check found
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Finding {
    /// The catalogue id (`EXE-3`)
    pub check: String,
    /// The catalogue's severity, or the project's override
    pub severity: Severity,
    pub confidence: Confidence,
    pub message: String,
    /// How to fix it
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fix: Option<String>,
    /// Where: the first span is the main one, the others are related places
    pub spans: Vec<Span>,
    /// What the finding is about, as a path that survives edits around it (`main::main::EXECUTOR_HIGH`,
    /// `.cargo/config.toml:target.thumbv6m-none-eabi`): with the check id it makes the fingerprint
    pub item: String,
    /// `<check>:<item>`, what waivers and baselines key on
    pub fingerprint: String,
    /// New or already in the baseline, when the run was compared with one
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub baseline: Option<BaselineState>,
    /// The reason of the source waiver that allows it; a waived finding never fails the run
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub waived: Option<String>,
}

impl Finding {
    /// A finding of `check` about `item`, at the catalogue's severity; certain until told otherwise
    pub fn new(check: &str, item: impl Into<String>, message: impl Into<String>) -> Finding {
        let item = item.into();
        let severity = crate::catalogue::catalogue()
            .entry(check)
            .map_or(Severity::Warning, |e| e.severity);
        Finding {
            check: check.to_string(),
            severity,
            confidence: Confidence::Certain,
            message: message.into(),
            fix: None,
            spans: Vec::new(),
            fingerprint: format!("{check}:{item}"),
            item,
            baseline: None,
            waived: None,
        }
    }

    pub fn span(mut self, span: Span) -> Finding {
        self.spans.push(span);
        self
    }

    pub fn fix(mut self, fix: impl Into<String>) -> Finding {
        self.fix = Some(fix.into());
        self
    }

    pub fn confidence(mut self, c: Confidence) -> Finding {
        self.confidence = c;
        self
    }

    /// One level lower than the catalogue says (an estimate of a weaker case)
    pub fn severity(mut self, s: Severity) -> Finding {
        self.severity = s;
        self
    }

    /// The main place
    pub fn primary(&self) -> Option<&Span> {
        self.spans.first()
    }
}

/// A finding against a baseline, matched by fingerprint
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum BaselineState {
    /// Not in the baseline
    New,
    /// In the baseline too
    Unchanged,
}

/// A registered check that did not run: which data was missing and what would provide it
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct NotRun {
    pub check: String,
    /// The missing data (`elf`, `build-script`)
    pub need: String,
    /// Why it is missing and what would enable the check
    pub reason: String,
}
