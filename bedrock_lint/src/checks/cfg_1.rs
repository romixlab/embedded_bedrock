//! `CFG-1` probe-rs runner or a recipe without `--probe VID:PID`

use crate::check::{Check, CheckMeta, Registered};
use crate::finding::{Finding, Span};
use crate::model::Firmware;

pub struct Cfg1;

static META: CheckMeta = CheckMeta {
    id: "CFG-1",
    needs: &[],
};

inventory::submit!(Registered(&Cfg1));

/// A runner line names a probe-rs subcommand that talks to a target (not `probe-rs list`)
fn is_probe_rs_target_command(v: &str) -> bool {
    v.contains("probe-rs")
        && ["run", "attach", "download", "reset", "erase", "dap-server"]
            .iter()
            .any(|cmd| v.split_whitespace().any(|w| w == *cmd))
}

/// `--probe` followed by a token starting with `VID:PID` (hex:hex)
fn has_probe_flag(v: &str) -> bool {
    let words: Vec<&str> = v.split_whitespace().collect();
    words
        .iter()
        .enumerate()
        .any(|(i, w)| *w == "--probe" && words.get(i + 1).is_some_and(|arg| is_vid_pid(arg)))
}

fn is_vid_pid(s: &str) -> bool {
    let vid_pid = s.split(':').take(2).collect::<Vec<_>>();
    vid_pid.len() == 2
        && vid_pid
            .iter()
            .all(|p| !p.is_empty() && p.chars().all(|c| c.is_ascii_hexdigit()))
}

const FIX: &str = "pass --probe VID:PID (probe-rs list) in the runner: with two probes on the PC another board gets flashed or erased";

impl Check for Cfg1 {
    fn meta(&self) -> &'static CheckMeta {
        &META
    }

    fn run(&self, fw: &Firmware) -> Vec<Finding> {
        let mut out = Vec::new();
        for c in &fw.cargo_configs {
            for (triple, table) in c.targets() {
                let Some(runner) = table.get("runner").and_then(|v| v.as_str()) else {
                    continue;
                };
                if is_probe_rs_target_command(runner) && !has_probe_flag(runner) {
                    let key = ["target", triple.as_str(), "runner"];
                    out.push(
                        Finding::new(
                            "CFG-1",
                            format!("target.{triple}.runner"),
                            format!(
                                "the runner for {triple} talks to a probe (`{runner}`) without --probe VID:PID"
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
