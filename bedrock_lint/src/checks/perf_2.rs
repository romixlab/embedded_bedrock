//! `PERF-2` 64-bit or i128 maths, float formatting, or a division by a runtime value on a core without them

use crate::check::{Check, CheckMeta, Need, Registered};
use crate::finding::{Confidence, Finding};
use crate::model::Firmware;

pub struct Perf2;

static META: CheckMeta = CheckMeta {
    id: "PERF-2",
    needs: &[Need::Elf, Need::Thumb],
};

inventory::submit!(Registered(&Perf2));

/// Software routines the compiler calls on a Cortex-M0+ (no divider, no 64-bit multiply), by what they do
const LIBCALLS: &[(&str, &[&str])] = &[
    ("a 64-bit multiply", &["__aeabi_lmul", "__muldi3"]),
    (
        "a 64-bit division",
        &[
            "__aeabi_uldivmod",
            "__aeabi_ldivmod",
            "__udivmoddi4",
            "__divmoddi4",
            "__udivdi3",
            "__divdi3",
            "__umoddi3",
            "__moddi3",
        ],
    ),
    ("an i128 multiply", &["__multi3"]),
    (
        "an i128 division",
        &[
            "__udivti3",
            "__umodti3",
            "__divti3",
            "__modti3",
            "__udivmodti4",
        ],
    ),
    (
        "a 32-bit division (no divider on the M0+)",
        &[
            "__aeabi_uidiv",
            "__aeabi_uidivmod",
            "__aeabi_idiv",
            "__aeabi_idivmod",
        ],
    ),
];

/// Families with a Cortex-M0 / M0+ core
const M0: &[&str] = &["stm32g0", "stm32c0", "stm32l0", "stm32f0", "rp2040"];

/// A routine of the runtime itself (its callers are not the firmware's hot paths)
fn is_runtime(name: &str) -> bool {
    name.starts_with("__")
        || name.starts_with("compiler_builtins::")
        || name.contains("core::fmt::")
}

impl Check for Perf2 {
    fn meta(&self) -> &'static CheckMeta {
        &META
    }

    /// On an M0 / M0+ (by the chip, else the build target `thumbv6m`): each group of routines that the firmware's
    /// own functions call, with the callers, and float formatting when it is linked in. Candidates: the tool can't
    /// tell a hot path from setup code.
    fn run(&self, fw: &Firmware) -> Vec<Finding> {
        let Some(elf) = &fw.elf else {
            return Vec::new();
        };
        let m0 = match fw.chip() {
            Some(c) => M0.contains(&c.family.as_str()),
            None => fw
                .build_target()
                .is_some_and(|(_, t)| t.starts_with("thumbv6m")),
        };
        if !m0 {
            return Vec::new();
        }
        let mut out = Vec::new();
        for (what, names) in LIBCALLS {
            let targets: Vec<usize> = (0..elf.symbols.len())
                .filter(|&i| names.contains(&elf.symbols[i].raw.as_str()))
                .collect();
            if targets.is_empty() {
                continue;
            }
            let mut callers: Vec<&str> = targets
                .iter()
                .flat_map(|&t| elf.callers(t))
                .map(|c| elf.symbols[c].name.as_str())
                .filter(|n| !is_runtime(n))
                .collect();
            callers.sort_unstable();
            callers.dedup();
            if callers.is_empty() {
                continue;
            }
            let first = elf.symbols[targets[0]].raw.clone();
            let shown: Vec<&str> = callers.iter().take(6).copied().collect();
            let more = if callers.len() > shown.len() {
                format!(" and {} more", callers.len() - shown.len())
            } else {
                String::new()
            };
            out.push(
                Finding::new(
                    "PERF-2",
                    format!("elf:libcall:{first}"),
                    format!(
                        "`{first}` ({what}, a software routine on this core) is called from {}{more}",
                        shown.join(", ")
                    ),
                )
                .span(fw.elf_span(first))
                .fix("on a hot path: 32-bit types, a shift or a multiply by a constant reciprocal instead of a division, or the work moved out of the per-element loop")
                .confidence(Confidence::Candidate),
            );
        }
        let float: u64 = elf
            .symbols
            .iter()
            .filter(|s| s.name.starts_with("core::fmt::float::"))
            .map(|s| s.size)
            .sum();
        if float > 0 {
            out.push(
                Finding::new(
                    "PERF-2",
                    "elf:core::fmt::float",
                    format!("float formatting (core::fmt::float, {float} B) is linked in: slow and large on a core without an FPU"),
                )
                .span(fw.elf_span("core::fmt::float"))
                .fix("send the raw value (defmt formats floats on the host) or format fixed-point integers")
                .confidence(Confidence::Candidate),
            );
        }
        out
    }
}
