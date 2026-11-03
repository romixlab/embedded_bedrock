//! `MEM-3` RAM init or zeroing range overlaps `.uninit`

use crate::check::{Check, CheckMeta, Need, Registered};
use crate::finding::{Confidence, Finding};
use crate::model::Firmware;

pub struct Mem3;

static META: CheckMeta = CheckMeta {
    id: "MEM-3",
    needs: &[Need::Elf],
};

inventory::submit!(Registered(&Mem3));

impl Check for Mem3 {
    fn meta(&self) -> &'static CheckMeta {
        &META
    }

    /// The ranges startup code writes: `.data` and `.bss` (cortex-m-rt's `__sdata`..`__edata`, `__sbss`..`__ebss`,
    /// else the sections), and the memory.x helper ranges `__<region>_start`..`__<region>_end` of RAM regions the
    /// source names (the template's `init_ram.rs` zeroes extra RAM banks through them: an estimate, the tool can't
    /// see what the code does with the range). Each against every section named `*uninit*`.
    fn run(&self, fw: &Firmware) -> Vec<Finding> {
        let Some(elf) = &fw.elf else {
            return Vec::new();
        };
        let pair = |a: &str, b: &str| Some((elf.symbol(a)?.addr, elf.symbol(b)?.addr));
        let section = |n: &str| elf.section(n).map(|s| (s.addr, s.end()));
        let mut ranges: Vec<(String, u64, u64, bool)> = Vec::new();
        if let Some((a, b)) = pair("__sdata", "__edata").or_else(|| section(".data")) {
            ranges.push((".data".into(), a, b, true));
        }
        if let Some((a, b)) = pair("__sbss", "__ebss").or_else(|| section(".bss")) {
            ranges.push((".bss".into(), a, b, true));
        }
        for r in fw.memory.iter().filter(|r| r.is_ram()) {
            let l = r.name.to_ascii_lowercase();
            let (start, end) = (format!("__{l}_start"), format!("__{l}_end"));
            let named = fw.sources.iter().any(|s| s.text.contains(&start));
            if named && let Some((a, b)) = pair(&start, &end) {
                ranges.push((format!("{start}..{end}"), a, b, false));
            }
        }
        let mut out = Vec::new();
        for u in elf.alloc_sections().filter(|s| s.name.contains("uninit")) {
            for (name, a, b, certain) in &ranges {
                if *a < u.end() && u.addr < *b {
                    let mut f = Finding::new(
                        "MEM-3",
                        format!("elf:{name}:{}", u.name),
                        format!(
                            "{name} (0x{a:08x}..0x{b:08x}) overlaps {} (0x{:08x}..0x{:08x}): startup {} what must survive the reset",
                            u.name,
                            u.addr,
                            u.end(),
                            if *certain { "overwrites" } else { "may zero" }
                        ),
                    )
                    .span(fw.elf_span(u.name.clone()))
                    .fix("keep .uninit out of every range the startup code initializes: shrink the range or place .uninit in a region nothing zeroes");
                    if !certain {
                        f = f.confidence(Confidence::Estimate);
                    }
                    out.push(f);
                }
            }
        }
        out
    }
}
