//! `SIZ-5` `core::fmt` pulled in

use crate::check::{Check, CheckMeta, Need, Registered};
use crate::config::Config;
use crate::finding::Finding;
use crate::model::Firmware;
use crate::model::elf::{SymKind, Symbol, bytes};

pub struct Siz5;

static META: CheckMeta = CheckMeta {
    id: "SIZ-5",
    needs: &[Need::Elf],
};

inventory::submit!(Registered(&Siz5));

/// Code of `core::fmt` itself or a `Debug` / `Display` impl
fn is_fmt(s: &Symbol) -> bool {
    s.kind == SymKind::Func
        && (s.name.starts_with("core::fmt::")
            || s.name.starts_with("<core::fmt::")
            || s.name.contains(" as core::fmt::"))
}

impl Check for Siz5 {
    fn meta(&self) -> &'static CheckMeta {
        &META
    }

    /// The ELF part: the size of the `core::fmt` code and, from the calls in Thumb code, the functions outside it
    /// that call into it (where it is pulled in from)
    fn run_with(&self, fw: &Firmware, cfg: &Config) -> Vec<Finding> {
        let Some(elf) = &fw.elf else {
            return Vec::new();
        };
        let fmt: Vec<usize> = (0..elf.symbols.len())
            .filter(|&i| is_fmt(&elf.symbols[i]))
            .collect();
        let total: u64 = fmt.iter().map(|&i| elf.symbols[i].size).sum();
        if fmt.is_empty() || total < cfg.limits.fmt_bytes {
            return Vec::new();
        }
        let mut from: Vec<&str> = Vec::new();
        for c in &elf.code {
            let s = &elf.symbols[c.symbol];
            if !is_fmt(s) && c.calls.iter().any(|t| fmt.contains(t)) {
                from.push(&s.name);
            }
        }
        from.sort_unstable();
        from.dedup();
        let shown: Vec<&str> = from.iter().take(8).copied().collect();
        let more = if from.len() > shown.len() {
            format!(" and {} more", from.len() - shown.len())
        } else {
            String::new()
        };
        let msg = if shown.is_empty() {
            format!(
                "core::fmt is linked in: {} functions, {}",
                fmt.len(),
                bytes(total)
            )
        } else {
            format!(
                "core::fmt is linked in: {} functions, {}, called from {}{more}",
                fmt.len(),
                bytes(total),
                shown.join(", ")
            )
        };
        vec![
            Finding::new("SIZ-5", "elf:core::fmt", msg)
                .span(fw.elf_span("core::fmt"))
                .fix("log with defmt instead of Debug / Display, `unwrap` / `expect` on Results through a match or `defmt::unwrap!`, a panic handler that doesn't format PanicInfo (panic-probe with print-defmt)"),
        ]
    }
}
