//! `CFG-5` Chip feature (package, flash size) of the HAL not the part on the board

use crate::check::{Check, CheckMeta, Need, Registered};
use crate::finding::Finding;
use crate::finding::Span;
use crate::model::Firmware;

pub struct Cfg5;

static META: CheckMeta = CheckMeta {
    id: "CFG-5",
    needs: &[Need::Chip],
};

inventory::submit!(Registered(&Cfg5));

impl Check for Cfg5 {
    fn meta(&self) -> &'static CheckMeta {
        &META
    }

    /// STM32 only for now: embassy-stm32's chip feature is the chip name, lower-case, exactly as `bedrock_fw.json`
    /// names it (`STM32G0B1RE` → `stm32g0b1re`)
    fn run(&self, fw: &Firmware) -> Vec<Finding> {
        let Some(chip) = fw.chip() else {
            return Vec::new();
        };
        if !chip.family.starts_with("stm32") {
            return Vec::new();
        }
        let expected = chip.name.to_ascii_lowercase();
        let mut out = Vec::new();
        for d in fw.dependencies() {
            if d.package() != "embassy-stm32" {
                continue;
            }
            for (feature, file, line) in d.features() {
                if feature.starts_with("stm32") && feature != expected {
                    out.push(
                        Finding::new(
                            "CFG-5",
                            format!("{}:embassy-stm32:{feature}", file.rel()),
                            format!(
                                "embassy-stm32's chip feature is `{feature}`, but bedrock_fw.json names the chip \
                                 {} ({expected})",
                                chip.name
                            ),
                        )
                        .span(Span::new(file.rel(), line))
                        .fix(format!(
                            "replace the `{feature}` feature of embassy-stm32 with `{expected}`, or correct \
                             bedrock_fw.json's answers.chip if the board really is a {feature}"
                        )),
                    );
                }
            }
        }
        out
    }
}
