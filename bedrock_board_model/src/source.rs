//! Source text, spans and diagnostics.

use std::ops::Range;
use std::path::PathBuf;

use serde::Serialize;

/// Byte range in the description's text.
pub type Span = Range<usize>;

/// A value with the place in the file it came from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Located<T> {
    pub value: T,
    pub span: Span,
}

impl<T> Located<T> {
    pub(crate) fn from_toml(s: toml::Spanned<T>) -> Self {
        let span = s.span();
        Self {
            value: s.into_inner(),
            span,
        }
    }
}

impl<T: std::fmt::Display> std::fmt::Display for Located<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.value.fmt(f)
    }
}

impl Located<String> {
    /// The value as `&str`.
    pub fn as_str(&self) -> &str {
        &self.value
    }
}

/// The description's file and text, for line numbers and snippets.
#[derive(Debug, Clone)]
pub struct Source {
    pub path: PathBuf,
    pub text: String,
}

impl Source {
    /// 1-based line and column (in characters) of a byte offset.
    pub fn line_col(&self, offset: usize) -> (usize, usize) {
        let offset = offset.min(self.text.len());
        let before = &self.text[..offset];
        let line = before.matches('\n').count() + 1;
        let line_start = before.rfind('\n').map_or(0, |i| i + 1);
        (line, before[line_start..].chars().count() + 1)
    }

    /// The text of the 1-based line `line`, without its newline.
    pub fn line_text(&self, line: usize) -> &str {
        self.text.lines().nth(line.saturating_sub(1)).unwrap_or("")
    }
}

/// How bad a diagnostic is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    /// Information: a check that could not run, and why.
    Note,
    /// Probably a mistake, or a fact the description does not state yet.
    Warning,
    /// The description is wrong or contradicts the chip or the PCB.
    Error,
}

/// One finding: what is wrong, where, and in which configurations.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Diagnostic {
    /// Stable kebab-case name of the check (`pin-twice`, `pcb-net`).
    pub code: &'static str,
    pub severity: Severity,
    pub message: String,
    /// Where in the description; `None` for a finding about the file as a whole.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub span: Option<Span>,
    /// What would fix it, or the facts that help (the pins that can carry a signal).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub help: Option<String>,
    /// The configurations it holds in (`rev A`, `rev A + rework 5`); empty for the file as a whole.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub configs: Vec<String>,
}

impl Diagnostic {
    pub(crate) fn new(code: &'static str, severity: Severity, message: impl Into<String>) -> Self {
        Self {
            code,
            severity,
            message: message.into(),
            span: None,
            help: None,
            configs: Vec::new(),
        }
    }

    pub(crate) fn error(code: &'static str, message: impl Into<String>) -> Self {
        Self::new(code, Severity::Error, message)
    }

    pub(crate) fn warning(code: &'static str, message: impl Into<String>) -> Self {
        Self::new(code, Severity::Warning, message)
    }

    pub(crate) fn at(mut self, span: &Span) -> Self {
        self.span = Some(span.clone());
        self
    }

    pub(crate) fn help(mut self, help: impl Into<String>) -> Self {
        self.help = Some(help.into());
        self
    }
}
