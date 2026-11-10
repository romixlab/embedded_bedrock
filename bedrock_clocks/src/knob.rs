//! A knob value its type can't hold, shared by the families' constructors.

use crate::source::Source;

/// A knob value its type can't hold, from a constructor such as `h72x::divm` or `g0::pllm`: what was asked, what
/// is allowed and where that is written. The front end adds the config key (`pll1.m`) to make it a finding.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct KnobError {
    /// The field within its table: `m`, `fracn`, `hpre`.
    pub field: &'static str,
    pub message: String,
    pub source: Source,
}

impl core::fmt::Display for KnobError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{}: {} [{}]", self.field, self.message, self.source)
    }
}

impl std::error::Error for KnobError {}
