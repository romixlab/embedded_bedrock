//! bedrock_lint: reads a bare-metal Rust firmware crate into a [`Firmware`] model and runs the automatic checks of
//! the firmware check catalogue (`book/checks.md`) on it.
//!
//! ```no_run
//! use bedrock_lint::{Firmware, LoadOptions, RunOptions, run};
//! let fw = Firmware::load("fw/my_board".as_ref(), &LoadOptions::default()).unwrap();
//! let report = run(&fw, &RunOptions::default()).unwrap();
//! for f in &report.findings {
//!     println!("{} {}: {}", f.check, f.primary().unwrap(), f.message);
//! }
//! ```
//!
//! - [`model`]: the firmware model (manifests, cargo configuration, build script, memory.x, bedrock_fw.json, source
//!   facts drawn with syn);
//! - [`check`]: the [`Check`] trait and the registry, one file per check under `checks/`;
//! - [`engine`]: a run, with not-run reasons, severity overrides, source waivers and the baseline;
//! - [`report`]: text, JSON and SARIF;
//! - [`catalogue`]: the catalogue, embedded at build time.

pub mod baseline;
pub mod catalogue;
pub mod check;
mod checks;
pub mod config;
pub mod engine;
pub mod finding;
pub mod model;
pub mod report;
pub mod waiver;

pub use baseline::Baseline;
pub use check::{Check, CheckMeta, Need, registered};
pub use config::Config;
pub use engine::{RunError, RunOptions, run, run_with};
pub use finding::{BaselineState, Confidence, Finding, NotRun, Severity, Span};
pub use model::{Firmware, LoadError, LoadOptions};
pub use report::{FailOn, Report};

/// The library's version, reported as the tool's
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
