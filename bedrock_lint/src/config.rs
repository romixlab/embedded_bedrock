//! `bedrock-lint.toml` in the firmware crate: the project's severity overrides, checks it skips (each with a
//! reason) and its baseline file.
//!
//! ```toml
//! baseline = "bedrock-lint.baseline.json"
//!
//! [severity]
//! "EXE-12" = "warning"
//!
//! [skip]
//! "STK-4" = "paint-stack is checked on the bench, see FEATURES PLAT-9"
//!
//! [limits]
//! region-percent = 95
//! frame = 4096
//! ```

use crate::finding::Severity;
use serde::Deserialize;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// The file name looked for in the crate directory
pub const FILE: &str = "bedrock-lint.toml";

#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    /// The baseline file, relative to the crate directory
    #[serde(default)]
    pub baseline: Option<PathBuf>,
    /// Severity per check id
    #[serde(default)]
    pub severity: BTreeMap<String, Severity>,
    /// Checks not run, with the reason
    #[serde(default)]
    pub skip: BTreeMap<String, String>,
    /// The thresholds of the elf checks
    #[serde(default)]
    pub limits: Limits,
}

/// Thresholds of the checks that measure something, with the defaults the catalogue names
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "kebab-case", default)]
pub struct Limits {
    /// SIZ-1: a memory region used over this percentage
    pub region_percent: u32,
    /// STK-6: one function's stack frame over this many bytes
    pub frame: u32,
    /// STK-5: a frame this large in a function that moves a value into a static
    pub move_frame: u32,
    /// SIZ-6: a `.data` initializer of this many bytes or more
    pub data_init: u64,
    /// SIZ-5: `core::fmt` code of this many bytes or more
    pub fmt_bytes: u64,
}

impl Default for Limits {
    fn default() -> Limits {
        Limits {
            region_percent: 90,
            frame: 2048,
            move_frame: 1024,
            data_init: 1024,
            fmt_bytes: 1024,
        }
    }
}

impl Config {
    /// Reads `bedrock-lint.toml` from `dir`; a missing file is the default
    pub fn load(dir: &Path) -> Result<Config, String> {
        let p = dir.join(FILE);
        let Ok(text) = std::fs::read_to_string(&p) else {
            return Ok(Config::default());
        };
        let c: Config = toml::from_str(&text).map_err(|e| format!("{}: {e}", p.display()))?;
        let cat = crate::catalogue::catalogue();
        for id in c.severity.keys().chain(c.skip.keys()) {
            if cat.entry(id).is_none() {
                return Err(format!(
                    "{}: {id} is not a check of the catalogue",
                    p.display()
                ));
            }
        }
        for (id, reason) in &c.skip {
            if reason.trim().is_empty() {
                return Err(format!("{}: skip of {id} needs a reason", p.display()));
            }
        }
        Ok(c)
    }
}
