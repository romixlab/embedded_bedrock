//! `MEM-1` `FLASH` length in memory.x covers a region kept for something else

use super::memx::regions;
use crate::check::{Check, CheckMeta, Need, Registered};
use crate::finding::{Finding, Span};
use crate::model::Firmware;

pub struct Mem1;

static META: CheckMeta = CheckMeta {
    id: "MEM-1",
    needs: &[Need::MemoryX],
};

inventory::submit!(Registered(&Mem1));

impl Check for Mem1 {
    fn meta(&self) -> &'static CheckMeta {
        &META
    }

    /// Any other named region (`SETTINGS`, `BOOTLOADER_STATE`, ...) whose start falls inside `FLASH`'s own span
    fn run(&self, fw: &Firmware) -> Vec<Finding> {
        let Some(mx) = &fw.memory_x else {
            return Vec::new();
        };
        let all = regions(&mx.text);
        let Some(flash) = all.iter().find(|r| r.name == "FLASH") else {
            return Vec::new();
        };
        let mut out = Vec::new();
        for r in &all {
            if r.name == "FLASH" || r.name == "RAM" {
                continue;
            }
            if r.origin >= flash.origin && r.origin < flash.origin + flash.length {
                out.push(
                    Finding::new(
                        "MEM-1",
                        format!("memory.x:{}", r.name),
                        format!(
                            "FLASH is ORIGIN = 0x{:08X}, LENGTH covers up to 0x{:08X}, which reaches into {} \
                             (starts at 0x{:08X}): the image can grow into it",
                            flash.origin,
                            flash.origin + flash.length,
                            r.name,
                            r.origin
                        ),
                    )
                    .span(Span::new(mx.rel.clone(), r.line))
                    .span(Span::new(mx.rel.clone(), flash.line).label("FLASH"))
                    .fix(format!(
                        "shrink FLASH's LENGTH so it ends at or before 0x{:08X} ({})",
                        r.origin, r.name
                    )),
                );
            }
        }
        out
    }
}
