//! `CFG-2` `probe-rs download` then `reset` on F0/G0/L0

use crate::check::{Check, CheckMeta, Need, Registered};
use crate::finding::{Finding, Span};
use crate::model::Firmware;

pub struct Cfg2;

static META: CheckMeta = CheckMeta {
    id: "CFG-2",
    needs: &[Need::Chip],
};

inventory::submit!(Registered(&Cfg2));

const AFFECTED: &[&str] = &["stm32f0", "stm32g0", "stm32l0"];

fn has_subcommand(segment: &str, cmd: &str) -> bool {
    segment.contains("probe-rs") && segment.split_whitespace().any(|w| w == cmd)
}

const FIX: &str = "probe-rs writes RCC_APBENR1 back on a running core (USB clock off) between a plain `download` and a \
    later `reset`; use a runner that halts the core first (`probe-rs run`, or `download` with `--chip` and a reset \
    that doesn't resume it, e.g. the same probe-rs invocation)";

impl Check for Cfg2 {
    fn meta(&self) -> &'static CheckMeta {
        &META
    }

    /// A runner (or a chain of them) that calls `probe-rs download` and, separately, `probe-rs reset`, on an F0, G0
    /// or L0 part
    fn run(&self, fw: &Firmware) -> Vec<Finding> {
        let Some(family) = fw.chip().map(|c| c.family.as_str()) else {
            return Vec::new();
        };
        if !AFFECTED.contains(&family) {
            return Vec::new();
        }
        let mut out = Vec::new();
        for c in &fw.cargo_configs {
            for (triple, table) in c.targets() {
                let Some(runner) = table.get("runner").and_then(|v| v.as_str()) else {
                    continue;
                };
                let segments: Vec<&str> = runner.split(['&', ';']).collect();
                let has_download = segments.iter().any(|s| has_subcommand(s, "download"));
                let has_reset = segments.iter().any(|s| has_subcommand(s, "reset"));
                if has_download && has_reset {
                    let key = ["target", triple.as_str(), "runner"];
                    out.push(
                        Finding::new(
                            "CFG-2",
                            format!("target.{triple}.runner"),
                            format!(
                                "the runner for {triple} chains `probe-rs download` and `reset` on {family}: \
                                 probe-rs resumes the core for the reset and writes RCC_APBENR1 back on it"
                            ),
                        )
                        .span(Span::new(c.rel(), c.line_of(&key)))
                        .fix(FIX),
                    );
                }
            }
        }
        out
    }
}
