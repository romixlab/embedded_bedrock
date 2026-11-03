//! `MEM-10` RWX LOAD segment or a writable symbol in `.rodata`

use crate::check::{Check, CheckMeta, Need, Registered};
use crate::finding::Finding;
use crate::model::Firmware;
use crate::model::elf::SymKind;

pub struct Mem10;

static META: CheckMeta = CheckMeta {
    id: "MEM-10",
    needs: &[Need::Elf],
};

inventory::submit!(Registered(&Mem10));

impl Check for Mem10 {
    fn meta(&self) -> &'static CheckMeta {
        &META
    }

    /// A LOAD segment that is writable and executable and runs from where it is stored, outside RAM; a writable section that runs where nothing can be written:
    /// `.rodata*` / `.text*`, a flash region of memory.x, or a LOAD segment without the write flag (a `static mut`
    /// with a `.rodata` link section makes the linker mark the whole output section writable). The finding sits on
    /// the first symbol of such a section that the source defines, so it can be waived there.
    fn run(&self, fw: &Firmware) -> Vec<Finding> {
        let Some(elf) = &fw.elf else {
            return Vec::new();
        };
        let mut out = Vec::new();
        // RAM functions make the RAM copy of .data writable and executable: on a chip without an MMU that is how
        // they work, so only a segment that runs where it is stored, outside RAM, counts
        let in_ram = |a: u64| fw.region_at(a).is_some_and(|r| r.is_ram());
        for g in elf
            .segments
            .iter()
            .filter(|g| g.write && g.exec && g.memsz > 0 && g.vaddr == g.paddr && !in_ram(g.vaddr))
        {
            out.push(
                Finding::new(
                    "MEM-10",
                    format!("elf:segment:{}", g.index),
                    format!(
                        "LOAD segment {} at 0x{:08x} is readable, writable and executable",
                        g.index, g.vaddr
                    ),
                )
                .span(fw.elf_span(format!("segment {}", g.index)))
                .fix("give code and data separate output sections in memory.x / the linker script so no segment is both writable and executable"),
            );
        }
        for s in elf.alloc_sections().filter(|s| s.write) {
            let read_only_name = s.name.starts_with(".rodata") || s.name.starts_with(".text");
            let in_flash = fw.region_at(s.addr).is_some_and(|r| r.is_flash());
            let in_ro_segment = elf.segments.iter().any(|g| {
                !g.write && g.memsz > 0 && s.addr >= g.vaddr && s.addr < g.vaddr + g.memsz
            });
            if !(read_only_name || in_flash || in_ro_segment) {
                continue;
            }
            let first = elf
                .symbols
                .iter()
                .filter(|y| y.kind == SymKind::Object && y.section.as_deref() == Some(&s.name))
                .map(|y| fw.symbol_span(y))
                .find(|sp| sp.line > 0);
            out.push(
                Finding::new(
                    "MEM-10",
                    format!("elf:section:{}", s.name),
                    format!(
                        "{} at 0x{:08x} is writable but runs from {}: a write there faults or does nothing",
                        s.name,
                        s.addr,
                        if in_flash { "flash" } else { "read-only memory" }
                    ),
                )
                .span(first.unwrap_or_else(|| fw.elf_span(s.name.clone())))
                .fix("a `static mut` (or a static with interior mutability) never gets a `.rodata` / `.text` link section: give it its own section in RAM, or make it a plain `static` of a read-only type"),
            );
        }
        out
    }
}
