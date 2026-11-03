//! `DMA-1` DMA buffer in memory the DMA master can't reach

use crate::check::{Check, CheckMeta, Need, Registered};
use crate::finding::{Confidence, Finding};
use crate::model::Firmware;
use crate::model::elf::SymKind;

pub struct Dma1;

static META: CheckMeta = CheckMeta {
    id: "DMA-1",
    needs: &[Need::Elf, Need::Chip],
};

inventory::submit!(Registered(&Dma1));

/// Memory the DMA controllers of a family can't reach, by family: on the H7 the DTCM (0x2000_0000, 128 KiB) sits on
/// the Cortex-M7's own bus and is reached from outside only through its AHBS port by the MDMA; DMA1 / DMA2, BDMA and
/// the bus-master peripherals (SDMMC IDMA, USB OTG, Ethernet) have no path to it (RM0433 / RM0468, system
/// architecture). A memory.x region named `DTCM*` overrides the default range.
const NO_DMA: &[(&str, &str, u64, u64)] = &[("stm32h7", "DTCM", 0x2000_0000, 0x2002_0000)];

impl Check for Dma1 {
    fn meta(&self) -> &'static CheckMeta {
        &META
    }

    /// Statics in such memory that look like DMA buffers: an estimate when their name or type says DMA (or is an
    /// SDMMC `DataBlock`), a candidate when it says buffer. The tool can't see which buffer reaches a DMA driver.
    fn run(&self, fw: &Firmware) -> Vec<Finding> {
        let (Some(elf), Some(chip)) = (&fw.elf, fw.chip()) else {
            return Vec::new();
        };
        let Some(&(_, mem, start, end)) = NO_DMA.iter().find(|(f, ..)| *f == chip.family) else {
            return Vec::new();
        };
        let (start, end) = fw
            .memory
            .iter()
            .find(|r| r.name.to_ascii_uppercase().starts_with(mem))
            .map_or((start, end), |r| (r.origin, r.end()));
        let mut out = Vec::new();
        for s in &elf.symbols {
            if s.kind != SymKind::Object || s.size == 0 || !(start..end).contains(&s.addr) {
                continue;
            }
            let ty = fw
                .facts
                .statics
                .iter()
                .find(|x| x.path == s.name)
                .map_or("", |x| x.ty.as_str());
            let text = format!("{} {ty}", s.name);
            let l = text.to_ascii_lowercase();
            let confidence = if l.contains("dma") || text.contains("DataBlock") {
                Confidence::Estimate
            } else if l.contains("buf") {
                Confidence::Candidate
            } else {
                continue;
            };
            out.push(
                Finding::new(
                    "DMA-1",
                    format!("elf:{}", s.name),
                    format!(
                        "`{}` is in {mem} (0x{:08x}), which DMA1 / DMA2, BDMA and the peripheral DMAs (SDMMC, USB OTG, Ethernet) can't reach",
                        s.name, s.addr
                    ),
                )
                .span(fw.symbol_span(s))
                .fix("put DMA buffers in AXI SRAM or SRAM1-3 with a link section (`#[unsafe(link_section = \".axisram\")]`), or move RAM out of DTCM in memory.x")
                .confidence(confidence),
            );
        }
        out
    }
}
