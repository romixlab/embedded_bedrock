//! `LOG-1` RTT that can block

use crate::check::{Check, CheckMeta, Need, Registered};
use crate::finding::{Confidence, Finding};
use crate::model::Firmware;
use crate::model::scan;

pub struct Log1;

static META: CheckMeta = CheckMeta {
    id: "LOG-1",
    needs: &[Need::Sources],
};

inventory::submit!(Registered(&Log1));

impl Check for Log1 {
    fn meta(&self) -> &'static CheckMeta {
        &META
    }

    /// probe-rs switches an RTT channel to blocking on attach, and a probe that stops reading (halted host, a
    /// closed `probe-rs run`) then stops the firmware in the logger:
    /// - `defmt_rtt` without its `disable-blocking-mode` feature: it blocks whenever the channel is in
    ///   BlockIfFull (`defmt-rtt` 1.3 `channel.rs`);
    /// - `BlockIfFull` named anywhere (a channel set up or switched to it);
    /// - rtt-target's `rtt_init_print!` / `rtt_init_default!` / `rtt_init_defmt!` without `ww_defmt_logger` (which
    ///   writes each frame whole or skips it, whatever the mode): an estimate, the channel starts NoBlockSkip and
    ///   only the probe changes it.
    fn run(&self, fw: &Firmware) -> Vec<Finding> {
        let words = scan::words(fw);
        let mut out = Vec::new();
        if let Some(w) = words.iter().find(|w| w.text == "defmt_rtt") {
            let off = fw
                .dependencies()
                .iter()
                .filter(|d| d.package() == "defmt-rtt")
                .any(|d| {
                    d.features()
                        .iter()
                        .any(|(f, _, _)| f == "disable-blocking-mode")
                });
            if !off {
                out.push(
                    Finding::new(
                        "LOG-1",
                        "defmt_rtt",
                        "defmt-rtt blocks when the probe sets its channel to blocking (probe-rs does on attach): a probe that stops reading stops the firmware",
                    )
                    .span(w.loc.span())
                    .fix("turn on defmt-rtt's disable-blocking-mode feature, or log through ww_defmt_logger (whole frames or skipped and counted)"),
                );
            }
        }
        if let Some(w) = words.iter().find(|w| w.text == "BlockIfFull") {
            out.push(
                Finding::new(
                    "LOG-1",
                    "BlockIfFull",
                    "an RTT channel in BlockIfFull: the firmware stops in the logger whenever the probe doesn't read",
                )
                .span(w.loc.span())
                .fix("use ChannelMode::NoBlockSkip and write whole frames or skip them (ww_defmt_logger does)"),
            );
        }
        let ww_logger = words.iter().any(|w| w.text == "ww_defmt_logger");
        if !ww_logger
            && let Some(w) = words.iter().find(|w| {
                matches!(
                    w.text.as_str(),
                    "rtt_init_print" | "rtt_init_default" | "rtt_init_defmt"
                )
            })
        {
            out.push(
                Finding::new(
                    "LOG-1",
                    w.text.clone(),
                    format!(
                        "{}! leaves the channel's mode to the probe: probe-rs sets it to blocking on attach, and the print then waits for the host",
                        w.text
                    ),
                )
                .span(w.loc.span())
                .fix("log through ww_defmt_logger, or set the channel back to NoBlockSkip before each write")
                .confidence(Confidence::Estimate),
            );
        }
        out
    }
}
