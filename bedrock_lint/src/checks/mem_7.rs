//! `MEM-7` Flash partition symbols for embassy-boot written as absolute addresses where offsets are expected

use super::memx::regions;
use crate::check::{Check, CheckMeta, Need, Registered};
use crate::finding::{Confidence, Finding, Span};
use crate::model::Firmware;

pub struct Mem7;

static META: CheckMeta = CheckMeta {
    id: "MEM-7",
    needs: &[Need::MemoryX],
};

inventory::submit!(Registered(&Mem7));

/// STM32's flash is always mapped at 0x08000000; this tells a flash partition (`BOOTLOADER_STATE`, `SETTINGS`)
/// apart from a RAM helper symbol (`ITCM`, `BKPSRAM`, ...), which never needs the subtraction
const FLASH_ADDR_SPACE: std::ops::Range<u64> = 0x0800_0000..0x1000_0000;

struct Const {
    symbol: String,
    region: String,
    /// The region named after `- ORIGIN(..)`, when the line subtracts one
    shift_base: Option<String>,
    line: u32,
}

/// `__name_start = ORIGIN(REGION) [- ORIGIN(BASE)];` and the `_end` form with `+ LENGTH(REGION)`
fn consts(text: &str) -> Vec<Const> {
    let mut out = Vec::new();
    for (i, line) in text.lines().enumerate() {
        let t = line.trim();
        if !t.starts_with("__") {
            continue;
        }
        let Some((lhs, rhs)) = t.split_once('=') else {
            continue;
        };
        let rhs = rhs.trim().trim_end_matches(';');
        let Some(after) = rhs.split_once("ORIGIN(").map(|(_, r)| r) else {
            continue;
        };
        let Some(region) = after.split(')').next() else {
            continue;
        };
        let shift_base = rhs
            .rsplit_once("- ORIGIN(")
            .and_then(|(_, r)| r.split(')').next());
        out.push(Const {
            symbol: lhs.trim().to_string(),
            region: region.to_string(),
            shift_base: shift_base.map(str::to_string),
            line: i as u32 + 1,
        });
    }
    out
}

const FIX: &str = "subtract the flash base's ORIGIN: embassy-boot and flash drivers read these as offsets from the \
    start of flash, not absolute addresses (`__name_start = ORIGIN(REGION) - ORIGIN(BASE);`)";

impl Check for Mem7 {
    fn meta(&self) -> &'static CheckMeta {
        &META
    }

    /// A flash partition's helper symbols written as an absolute address while a sibling partition's are offsets
    /// from the same base: the file's own convention, not outside knowledge of which names embassy-boot reads.
    /// RAM helper symbols (never shifted) are told apart from flash partitions by STM32's fixed flash address space.
    fn run(&self, fw: &Firmware) -> Vec<Finding> {
        let Some(mx) = &fw.memory_x else {
            return Vec::new();
        };
        let all = consts(&mx.text);
        let region_origin: std::collections::HashMap<String, u64> = regions(&mx.text)
            .into_iter()
            .map(|r| (r.name, r.origin))
            .collect();
        let bases: std::collections::HashSet<&str> =
            all.iter().filter_map(|c| c.shift_base.as_deref()).collect();
        let partitions: Vec<&Const> = all
            .iter()
            .filter(|c| {
                c.region != "FLASH"
                    && !bases.contains(c.region.as_str())
                    && region_origin
                        .get(c.region.as_str())
                        .is_some_and(|o| FLASH_ADDR_SPACE.contains(o))
            })
            .collect();
        if !partitions.iter().any(|c| c.shift_base.is_some()) {
            return Vec::new();
        }
        partitions
            .iter()
            .filter(|c| c.shift_base.is_none())
            .map(|c| {
                Finding::new(
                    "MEM-7",
                    format!("memory.x:{}", c.symbol),
                    format!(
                        "{} is an absolute address (ORIGIN({})) while a sibling partition's symbols subtract \
                         a flash base's ORIGIN",
                        c.symbol, c.region
                    ),
                )
                .span(Span::new(mx.rel.clone(), c.line))
                .fix(FIX)
                .confidence(Confidence::Estimate)
            })
            .collect()
    }
}
