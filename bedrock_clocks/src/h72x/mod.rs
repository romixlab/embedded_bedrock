//! The RM0468 family: STM32H723/733, H725/735 and H730.
//!
//! [plan] runs the checks and, when no error is left, evaluates the tree and encodes the registers;
//! [emit::emit] turns the result into `clocks.rs`.

pub mod check;
pub mod config;
pub mod emit;
pub mod limits;
pub mod regs;
pub mod tree;

pub use check::{Finding, Severity, check};
pub use config::*;
pub use regs::Regs;
pub use tree::Tree;

/// A config that passed the checks: its tree, its register words, and the warnings it still has.
#[derive(Clone, Debug)]
pub struct Planned {
    pub tree: Tree,
    pub regs: Regs,
    pub warnings: Vec<Finding>,
}

/// Checks `cfg`; on success the tree and the registers, else every finding (errors first).
pub fn plan(cfg: &Config) -> Result<Planned, Vec<Finding>> {
    let findings = check(cfg);
    if findings.iter().any(|f| f.severity == Severity::Error) {
        return Err(findings);
    }
    let tree = tree::evaluate(cfg).map_err(|e| {
        vec![Finding {
            severity: Severity::Error,
            key: "config".into(),
            message: e.to_string(),
            source: None,
        }]
    })?;
    let regs = regs::encode(cfg, &tree);
    Ok(Planned {
        tree,
        regs,
        warnings: findings,
    })
}
