//! The checks of a description, run on every configuration and merged.

use std::collections::BTreeMap;

use crate::chip::ChipData;
use crate::load::Description;
use crate::pcb::McuPins;
use crate::source::{Diagnostic, Severity};

mod chip;
mod pcb;
mod pins;

/// The outcome of [`check()`].
#[derive(Debug, Clone)]
pub struct Report {
    /// The labels of the configurations checked, in order.
    pub configs: Vec<String>,
    /// Every finding once, with the configurations it holds in; ordered by place in the file.
    pub diagnostics: Vec<Diagnostic>,
}

impl Report {
    /// How many findings have `severity`.
    pub fn count(&self, severity: Severity) -> usize {
        self.diagnostics
            .iter()
            .filter(|d| d.severity == severity)
            .count()
    }

    /// The worst severity found.
    pub fn worst(&self) -> Option<Severity> {
        self.diagnostics.iter().map(|d| d.severity).max()
    }
}

/// Checks every configuration of `desc`: pins used twice and shorted pins both used (always), pins and signals
/// against `chip` (when given), and against the PCB of each configuration when `pcbs` has it (keyed by the
/// `pcb` value as the description writes it).
pub fn check(
    desc: &Description,
    chip: Option<&ChipData>,
    pcbs: &BTreeMap<String, McuPins>,
) -> Report {
    let mut merged: Vec<Diagnostic> = Vec::new();
    if let Some(chip) = chip {
        chip::package(&desc.board, chip, &mut merged);
    }
    let configs = desc.configurations();
    for board in configs {
        let mut found = Vec::new();
        pins::check(board, &mut found);
        if let Some(chip) = chip {
            chip::check(board, &desc.board, chip, &mut found);
        }
        if let Some(mcu) = board.pcb.as_ref().and_then(|p| pcbs.get(&p.value)) {
            pcb::check(board, mcu, &mut found);
        }
        for mut d in found {
            match merged.iter_mut().find(|m| same(m, &d)) {
                Some(m) => m.configs.push(board.label.clone()),
                None => {
                    d.configs = vec![board.label.clone()];
                    merged.push(d);
                }
            }
        }
    }
    merged.sort_by(|a, b| {
        let key = |d: &Diagnostic| d.span.as_ref().map_or(usize::MAX, |s| s.start);
        key(a)
            .cmp(&key(b))
            .then(a.code.cmp(b.code))
            .then(a.message.cmp(&b.message))
    });
    Report {
        configs: configs.iter().map(|b| b.label.clone()).collect(),
        diagnostics: merged,
    }
}

fn same(a: &Diagnostic, b: &Diagnostic) -> bool {
    a.code == b.code
        && a.severity == b.severity
        && a.message == b.message
        && a.span == b.span
        && a.help == b.help
}
