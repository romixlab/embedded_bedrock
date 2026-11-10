//! What the checkers of every family report: one problem with a config, with its key, value, range and source.

use core::fmt;

use crate::source::Source;

#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Severity {
    Warning,
    Error,
}

/// One problem with a config.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Finding {
    pub severity: Severity,
    /// The config key it is about: `pll1.n`, `sys.hpre`, `kernel.fdcan`.
    pub key: String,
    pub message: String,
    pub source: Option<Source>,
}

impl fmt::Display for Finding {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let sev = match self.severity {
            Severity::Error => "error",
            Severity::Warning => "warning",
        };
        write!(f, "{sev}: {}: {}", self.key, self.message)?;
        if let Some(s) = &self.source {
            write!(f, " [{s}]")?;
        }
        Ok(())
    }
}
