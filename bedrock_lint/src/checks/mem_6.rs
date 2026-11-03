//! `MEM-6` `.cnt_bkp_buffer` not at the backup-register address

use crate::check::{Check, CheckMeta, Need, Registered};
use crate::finding::{Confidence, Finding};
use crate::model::Firmware;

pub struct Mem6;

static META: CheckMeta = CheckMeta {
    id: "MEM-6",
    needs: &[Need::Elf],
};

inventory::submit!(Registered(&Mem6));

/// The first backup register per chip family, as the template's generator has them
/// (`firmware_template_skill/scripts/bedrock_gen.py`): G0 `TAMP_BKP0R` (RM0444 §31.6.12), H7 `RTC_BKP0R` (RM0468
/// §51.7.20, RM0433)
const BKP: &[(&str, u64, &str)] = &[
    ("stm32g0", 0x4000_B100, "TAMP_BKP0R"),
    ("stm32h7", 0x5800_4050, "RTC_BKP0R"),
];

impl Check for Mem6 {
    fn meta(&self) -> &'static CheckMeta {
        &META
    }

    /// Certain against the chip table; for other chips an estimate when the buffer is in RAM (or flash), where it
    /// neither survives a reset the way the counters expect nor is a register
    fn run(&self, fw: &Firmware) -> Vec<Finding> {
        let Some(elf) = &fw.elf else {
            return Vec::new();
        };
        let Some(s) = elf.section(".cnt_bkp_buffer").filter(|s| s.size > 0) else {
            return Vec::new();
        };
        let span = match fw.region_at(s.addr) {
            Some(r) => fw.region_span(r),
            None => fw.elf_span(".cnt_bkp_buffer"),
        };
        let fix = "place the BKP counters' region (BKPSRAM / BKP_REGS in memory.x) at the chip's first backup register, as the template generates it";
        let family = fw.chip().map(|c| c.family.as_str());
        if let Some((_, want, reg)) = BKP.iter().find(|(f, _, _)| Some(*f) == family) {
            if s.addr != *want {
                return vec![
                    Finding::new(
                        "MEM-6",
                        "elf:.cnt_bkp_buffer",
                        format!(
                            ".cnt_bkp_buffer is at 0x{:08x}, the backup registers start at 0x{want:08x} ({reg})",
                            s.addr
                        ),
                    )
                    .span(span)
                    .fix(fix),
                ];
            }
            return Vec::new();
        }
        let in_memory = fw
            .region_at(s.addr)
            .is_some_and(|r| r.is_ram() || r.is_flash())
            || (0x2000_0000..0x4000_0000).contains(&s.addr);
        if in_memory {
            return vec![
                Finding::new(
                    "MEM-6",
                    "elf:.cnt_bkp_buffer",
                    format!(
                        ".cnt_bkp_buffer is at 0x{:08x}, in memory, not in backup registers",
                        s.addr
                    ),
                )
                .span(span)
                .fix(fix)
                .confidence(Confidence::Estimate),
            ];
        }
        Vec::new()
    }
}
