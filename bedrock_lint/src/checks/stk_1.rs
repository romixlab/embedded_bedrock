//! `STK-1` flip-link not the linker

use crate::check::{Check, CheckMeta, Registered};
use crate::finding::{Confidence, Finding, Span};
use crate::model::{Firmware, TomlFile};

pub struct Stk1;

static META: CheckMeta = CheckMeta {
    id: "STK-1",
    needs: &[],
};

inventory::submit!(Registered(&Stk1));

/// flip-link named as the linker (`linker = "flip-link"`, a path ending in it, or `-C linker=flip-link` in rustflags)
fn is_flip_link(v: &str) -> bool {
    let v = v.trim();
    v == "flip-link" || v.ends_with("/flip-link") || v.ends_with("\\flip-link.exe")
}

fn rustflags_name_it(v: Option<&toml::Value>) -> bool {
    let flags: Vec<String> = match v {
        Some(toml::Value::String(s)) => s.split_whitespace().map(str::to_string).collect(),
        Some(toml::Value::Array(a)) => a
            .iter()
            .filter_map(|f| f.as_str().map(str::to_string))
            .collect(),
        _ => return false,
    };
    flags.iter().enumerate().any(|(i, f)| {
        let arg = f
            .strip_prefix("-Clinker=")
            .or_else(|| {
                (f == "-C")
                    .then(|| flags.get(i + 1).map(String::as_str))
                    .flatten()?
                    .strip_prefix("linker=")
            })
            .or_else(|| f.strip_prefix("linker="));
        arg.is_some_and(is_flip_link)
    })
}

/// Whether some config section for `target` (the triple's or a `cfg(..)` one) sets flip-link
fn configured(fw: &Firmware, target: &str) -> bool {
    if let Some((_, v)) = fw.cargo_config(&["target", target, "linker"]) {
        return v.as_str().is_some_and(is_flip_link);
    }
    if rustflags_name_it(
        fw.cargo_config(&["target", target, "rustflags"])
            .map(|(_, v)| v),
    ) || rustflags_name_it(fw.cargo_config(&["build", "rustflags"]).map(|(_, v)| v))
    {
        return true;
    }
    // `[target.'cfg(target_arch = "arm")']`: cfg expressions are not evaluated, any that names flip-link counts
    fw.cargo_configs.iter().any(|c| {
        c.get(&["target"])
            .and_then(|t| t.as_table())
            .is_some_and(|t| {
                t.iter().any(|(k, v)| {
                    k.starts_with("cfg(")
                        && (v
                            .get("linker")
                            .and_then(|l| l.as_str())
                            .is_some_and(is_flip_link)
                            || rustflags_name_it(v.get("rustflags")))
                })
            })
    })
}

fn opted_out(fw: &Firmware) -> &'static str {
    match fw
        .bedrock_fw
        .as_ref()
        .and_then(|b| b.answer("no_flip_link"))
    {
        Some(serde_json::Value::Bool(true)) => {
            " bedrock_fw.json records no_flip_link: when that is on purpose, waive it with the reason"
        }
        _ => "",
    }
}

const FIX: &str = "add `linker = \"flip-link\"` under `[target.<triple>]` in .cargo/config.toml (cargo install flip-link); a `cargo:rustc-linker` line in build.rs does nothing (STK-2)";

impl Check for Stk1 {
    fn meta(&self) -> &'static CheckMeta {
        &META
    }

    /// The target is `[build] target` of the cargo configuration, else every `[target.<triple>]` section there. A
    /// firmware without any cargo configuration links with the default linker.
    fn run(&self, fw: &Firmware) -> Vec<Finding> {
        if fw.cargo_configs.is_empty() {
            return vec![
                Finding::new(
                    "STK-1",
                    ".cargo/config.toml",
                    format!(
                        "no .cargo/config.toml: the firmware links with rust-lld, a stack overflow runs into .data and .bss unnoticed.{}",
                        opted_out(fw)
                    ),
                )
                .span(Span::new("Cargo.toml", 0))
                .fix(FIX),
            ];
        }
        if let Some((file, target)) = fw.build_target() {
            if configured(fw, &target) {
                return Vec::new();
            }
            let key = ["target", target.as_str(), "linker"];
            let (span, what) = match fw.cargo_config(&key) {
                Some((f, v)) => (
                    Span::new(f.rel(), f.line_of(&key)),
                    format!(
                        "the linker for {target} is `{}`, not flip-link",
                        v.as_str().unwrap_or("?")
                    ),
                ),
                None => (
                    target_span(fw, file, &target),
                    format!("no linker set for {target}: it links with rust-lld, not flip-link"),
                ),
            };
            return vec![
                Finding::new(
                    "STK-1",
                    format!("target.{target}.linker"),
                    format!(
                        "{what}; a stack overflow runs into .data and .bss unnoticed.{}",
                        opted_out(fw)
                    ),
                )
                .span(span)
                .fix(FIX),
            ];
        }
        // No build target: every embedded target section must name flip-link
        let mut out = Vec::new();
        let mut any = false;
        for c in &fw.cargo_configs {
            for (triple, _) in c.targets() {
                if triple.starts_with("cfg(") {
                    continue;
                }
                let triple = triple.as_str();
                any = true;
                if !configured(fw, triple) {
                    out.push(
                        Finding::new(
                            "STK-1",
                            format!("target.{triple}.linker"),
                            format!("no flip-link linker for {triple}.{}", opted_out(fw)),
                        )
                        .span(Span::new(c.rel(), c.line_of_table(&["target", triple])))
                        .fix(FIX),
                    );
                }
            }
        }
        if !any && !configured(fw, "") {
            let c = &fw.cargo_configs[0];
            out.push(
                Finding::new(
                    "STK-1",
                    "target.linker",
                    format!(
                        "{} sets no build target and no target section: flip-link is not the linker.{}",
                        c.rel(),
                        opted_out(fw)
                    ),
                )
                .span(Span::new(c.rel(), 0))
                .fix(FIX)
                .confidence(Confidence::Estimate),
            );
        }
        out
    }
}

/// The `[target.<triple>]` section, else the `[build] target` line
fn target_span(fw: &Firmware, build_file: &TomlFile, target: &str) -> Span {
    for c in &fw.cargo_configs {
        let l = c.line_of_table(&["target", target]);
        if l > 0 {
            return Span::new(c.rel(), l);
        }
    }
    Span::new(build_file.rel(), build_file.line_of(&["build", "target"]))
}
