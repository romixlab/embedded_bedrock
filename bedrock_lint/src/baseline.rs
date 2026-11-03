//! The baseline: the findings a firmware already had when it adopted the lint, by fingerprint, so a run fails only
//! on new ones. Written with `--write-baseline`, read with `--baseline` or `baseline =` in `bedrock-lint.toml`.

use crate::finding::{BaselineState, Finding};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::Path;

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Baseline {
    /// Format version
    pub version: u32,
    pub findings: Vec<BaselineEntry>,
}

/// One known finding; the message is for the reader of the file, matching is by fingerprint
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct BaselineEntry {
    pub fingerprint: String,
    pub check: String,
    pub message: String,
}

impl Baseline {
    pub const VERSION: u32 = 1;

    /// The findings of a run that are not waived
    pub fn from_findings(findings: &[Finding]) -> Baseline {
        let mut findings: Vec<BaselineEntry> = findings
            .iter()
            .filter(|f| f.waived.is_none())
            .map(|f| BaselineEntry {
                fingerprint: f.fingerprint.clone(),
                check: f.check.clone(),
                message: f.message.clone(),
            })
            .collect();
        findings.sort_by(|a, b| a.fingerprint.cmp(&b.fingerprint));
        Baseline {
            version: Self::VERSION,
            findings,
        }
    }

    pub fn load(p: &Path) -> Result<Baseline, String> {
        let text = std::fs::read_to_string(p).map_err(|e| format!("{}: {e}", p.display()))?;
        let b: Baseline =
            serde_json::from_str(&text).map_err(|e| format!("{}: {e}", p.display()))?;
        if b.version != Self::VERSION {
            return Err(format!(
                "{}: baseline version {} (this bedrock-lint reads {})",
                p.display(),
                b.version,
                Self::VERSION
            ));
        }
        Ok(b)
    }

    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(self).expect("baseline serializes") + "\n"
    }

    /// Marks each finding new or unchanged; a fingerprint in the baseline n times matches n findings
    pub fn mark(&self, findings: &mut [Finding]) {
        let mut left: BTreeMap<&str, usize> = BTreeMap::new();
        for e in &self.findings {
            *left.entry(&e.fingerprint).or_default() += 1;
        }
        for f in findings {
            let n = left.get_mut(f.fingerprint.as_str());
            f.baseline = Some(match n {
                Some(n) if *n > 0 => {
                    *n -= 1;
                    BaselineState::Unchanged
                }
                _ => BaselineState::New,
            });
        }
    }
}
