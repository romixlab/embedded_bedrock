//! `SIZ-1` Flash or RAM use over a threshold (default 90 % of the region)

use crate::check::{Check, CheckMeta, Need, Registered};
use crate::config::Config;
use crate::finding::Finding;
use crate::model::Firmware;
use crate::model::elf::bytes;

pub struct Siz1;

static META: CheckMeta = CheckMeta {
    id: "SIZ-1",
    needs: &[Need::Elf, Need::Regions],
};

inventory::submit!(Registered(&Siz1));

impl Check for Siz1 {
    fn meta(&self) -> &'static CheckMeta {
        &META
    }

    /// Per memory.x region (not register regions, not a region one section fills exactly, like BOOT2): the sections that run there, and the initial values of `.data` stored there. The
    /// stack is not counted: with flip-link it takes what is left of its RAM (STK-6 compares it with the calls).
    fn run_with(&self, fw: &Firmware, cfg: &Config) -> Vec<Finding> {
        let Some(elf) = &fw.elf else {
            return Vec::new();
        };
        let limit = cfg.limits.region_percent as u64;
        let mut out = Vec::new();
        for r in &fw.memory {
            if r.length == 0 || r.is_registers() {
                continue;
            }
            let mut used = 0;
            for s in elf.alloc_sections() {
                // what runs here, and the initial values of .data stored here
                if r.contains(s.addr) || (s.is_init_data() && r.contains(s.load)) {
                    used += s.size;
                }
            }
            // a region made for one fixed-size blob (the RP2040's BOOT2) is full by design
            let one_blob = elf
                .alloc_sections()
                .filter(|s| r.contains(s.addr))
                .map(|s| s.size)
                .eq([r.length]);
            if used * 100 >= limit * r.length && !one_blob {
                out.push(
                    Finding::new(
                        "SIZ-1",
                        format!("memory.x:{}", r.name),
                        format!(
                            "{} is {} % used: {} of {} in {}",
                            r.name,
                            used * 100 / r.length,
                            bytes(used),
                            bytes(r.length),
                            elf.rel
                        ),
                    )
                    .span(fw.region_span(r))
                    .fix(format!(
                        "make room before it runs out (limit {limit} %, `[limits] region-percent` in bedrock-lint.toml): opt-level \"z\" or \"s\" for flash, core::fmt out (SIZ-5), big statics and buffers smaller"
                    )),
                );
            }
        }
        out
    }
}
