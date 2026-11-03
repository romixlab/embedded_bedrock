//! `STK-3` Stack not below `.data` / `.bss` in the ELF, or a zero-size stack

use crate::check::{Check, CheckMeta, Need, Registered};
use crate::finding::Finding;
use crate::model::Firmware;
use crate::model::elf::bytes;

pub struct Stk3;

static META: CheckMeta = CheckMeta {
    id: "STK-3",
    needs: &[Need::Elf],
};

inventory::submit!(Registered(&Stk3));

impl Check for Stk3 {
    fn meta(&self) -> &'static CheckMeta {
        &META
    }

    /// cortex-m-rt's `_stack_start` is the initial stack pointer; the stack grows down from it. With flip-link it is
    /// the start of the lowest RAM section and the stack has the RAM below; without, it is the end of RAM and an
    /// overflow runs into `.bss` / `.data` silently. The RAM sections are the writable ones in the memory.x region
    /// of the stack (without memory.x: in the same 16 MiB as the stack).
    fn run(&self, fw: &Firmware) -> Vec<Finding> {
        let Some(elf) = &fw.elf else {
            return Vec::new();
        };
        let Some(top) = elf.symbol("_stack_start").map(|s| s.addr) else {
            return Vec::new();
        };
        // the region below the stack's start, or the one it starts at when it has no room
        let region = fw
            .region_at(top.saturating_sub(1))
            .or_else(|| fw.region_at(top));
        let same_ram = |addr: u64| match region {
            Some(r) => r.contains(addr),
            None => addr >> 24 == top.saturating_sub(1) >> 24,
        };
        let span = || match region {
            Some(r) => fw.region_span(r),
            None => fw.elf_span("_stack_start"),
        };
        let mut out = Vec::new();
        let below: Vec<_> = elf
            .alloc_sections()
            .filter(|s| s.write && same_ram(s.addr) && s.addr < top)
            .collect();
        if !below.is_empty() {
            let names: Vec<String> = below
                .iter()
                .map(|s| format!("{} at 0x{:08x}", s.name, s.addr))
                .collect();
            out.push(
                Finding::new(
                    "STK-3",
                    "elf:_stack_start",
                    format!(
                        "the stack starts at 0x{top:08x}, above {}: an overflow runs into them without a fault",
                        names.join(", ")
                    ),
                )
                .span(span())
                .fix("link with flip-link (`linker = \"flip-link\"` under `[target.<triple>]` in .cargo/config.toml, STK-1) so the stack sits below .data / .bss"),
            );
            return out;
        }
        let bottom = region
            .map(|r| r.origin)
            .or_else(|| elf.symbol("_stack_end").map(|s| s.addr));
        if let Some(bottom) = bottom
            && top <= bottom
        {
            out.push(
                Finding::new(
                    "STK-3",
                    "elf:_stack_start",
                    format!("the stack has no room: it starts at 0x{top:08x}, the bottom of its RAM"),
                )
                .span(span())
                .fix(format!(
                    "make room for the stack: RAM sections take the whole region ({}); check for an empty section at the end of RAM (flip-link 0.1.12) and for oversized statics",
                    region.map_or_else(String::new, |r| bytes(r.length))
                )),
            );
        }
        out
    }
}
