//! `SIZ-5` `core::fmt` pulled in

use crate::check::{Check, CheckMeta, Need, Registered};
use crate::config::Config;
use crate::finding::Finding;
use crate::model::Firmware;
use crate::model::elf::{SymKind, Symbol, bytes};
use crate::model::scan::{self, Node};
use syn::Expr;

pub struct Siz5;

static META: CheckMeta = CheckMeta {
    id: "SIZ-5",
    // the source part runs without an ELF, the ELF part when `--elf` gives one
    needs: &[Need::Sources],
};

inventory::submit!(Registered(&Siz5));

/// Calls that return a `Result` for certain, so an `.unwrap()` / `.expect()` on them formats the error with
/// `Debug` (embassy-boot's `mark_booted`, `try_into`, `parse`, ...); task functions too
const RESULT_CALLS: &[&str] = &[
    "try_into",
    "try_from",
    "parse",
    "map_err",
    "ok_or",
    "ok_or_else",
    "mark_booted",
    "mark_updated",
    "spawn",
];

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

    /// Both parts: the source's ways into `core::fmt` always, the linked code when there is an ELF
    fn run_with(&self, fw: &Firmware, cfg: &Config) -> Vec<Finding> {
        let mut out = source(fw);
        out.extend(elf(fw, cfg));
        out
    }
}

/// The ELF part: the size of the `core::fmt` code and, from the calls in Thumb code, the functions outside it that
/// call into it (where it is pulled in from)
fn elf(fw: &Firmware, cfg: &Config) -> Vec<Finding> {
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
        // OUTLINED_FUNCTION_n: code the compiler factored out, not a place in the firmware
        if !is_fmt(s)
            && !s.name.starts_with("OUTLINED_FUNCTION")
            && c.calls.iter().any(|t| fmt.contains(t))
        {
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

/// The source part: what pulls `core::fmt` in (4-5 KiB of formatting machinery; the ELF half, LINT-5, looks for
/// the symbols): `Debug2Format` / `Display2Format`, a `#[panic_handler]` that formats its `PanicInfo`, and
/// `Result::unwrap` / `expect` (through `&dyn Debug`), found where the receiver is a call that returns a Result
/// for certain. `Option::unwrap` and defmt's `unwrap!` don't format; without types the other unwraps are not
/// judged.
fn source(fw: &Firmware) -> Vec<Finding> {
    let mut out = Vec::new();
    for w in scan::words(fw) {
        if w.text == "Debug2Format" || w.text == "Display2Format" {
            out.push(
                Finding::new("SIZ-5", format!("{}:{}", w.loc.file, w.text), format!("{} pulls in core::fmt (4-5 KiB)", w.text))
                    .span(w.loc.span())
                    .fix("log the fields with defmt formats instead (`{=u32:x}`, a defmt::Format derive)"),
            );
        }
    }
    for f in scan::functions(fw) {
        if f.has_attr("panic_handler") {
            let formats = scan::words_in(&f).iter().any(|w| {
                matches!(
                    w.text.as_str(),
                    "message"
                        | "location"
                        | "Display2Format"
                        | "Debug2Format"
                        | "write"
                        | "writeln"
                )
            }) || {
                let mut fmt = false;
                scan::each(f.block, |n| {
                    if let Node::Macro(name, m) = n
                        && f.params().iter().any(|(names, _)| {
                            names
                                .iter()
                                .any(|p| scan::text(&m.tokens).contains(p.as_str()))
                        })
                        && name != "unreachable"
                    {
                        fmt = true;
                    }
                });
                fmt
            };
            if formats {
                out.push(
                    Finding::new(
                        "SIZ-5",
                        format!("{}:panic_handler", f.path()),
                        "the panic handler formats its PanicInfo: core::fmt comes in with it (4-5 KiB)",
                    )
                    .span(f.loc().span())
                    .fix("log a fixed message and the location's file and line through defmt, or use panic-probe with print-defmt"),
                );
            }
        }
        scan::each(f.block, |n| {
            if let Node::Expr(Expr::MethodCall(m)) = n
                && (m.method == "unwrap" || m.method == "expect")
                && let Some(inner) = scan::call_name(&m.receiver)
            {
                let last = inner
                    .trim_start_matches('.')
                    .rsplit("::")
                    .next()
                    .unwrap_or_default();
                // an embassy task function returns Result<SpawnToken, SpawnError> (embassy-executor 0.9)
                let task = fw.facts.tasks.iter().any(|t| t.name == last);
                if RESULT_CALLS.contains(&last) || task {
                    out.push(
                        Finding::new(
                            "SIZ-5",
                            format!("{}:{last}.{}", f.path(), m.method),
                            format!(
                                "Result::{} on {last}(): the error is formatted through &dyn Debug, which pulls in core::fmt (4-5 KiB)",
                                m.method
                            ),
                        )
                        .span(scan::loc(f.file, m.method.span()).span())
                        .fix("defmt::unwrap!(..), or match the error and log it with defmt"),
                    );
                }
            }
        });
    }
    out
}
