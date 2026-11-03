//! `SIZ-6` Large `.data` initializer

use crate::check::{Check, CheckMeta, Need, Registered};
use crate::config::Config;
use crate::finding::Finding;
use crate::model::Firmware;
use crate::model::elf::{SymKind, bytes};

pub struct Siz6;

static META: CheckMeta = CheckMeta {
    id: "SIZ-6",
    needs: &[Need::Elf],
};

inventory::submit!(Registered(&Siz6));

impl Check for Siz6 {
    fn meta(&self) -> &'static CheckMeta {
        &META
    }

    /// Objects in a section that is copied from flash at startup, of `[limits] data-init` bytes or more
    fn run_with(&self, fw: &Firmware, cfg: &Config) -> Vec<Finding> {
        let Some(elf) = &fw.elf else {
            return Vec::new();
        };
        let mut out = Vec::new();
        for s in &elf.symbols {
            if s.kind != SymKind::Object || s.size < cfg.limits.data_init {
                continue;
            }
            let Some(sec) = s.section.as_ref().and_then(|n| elf.section(n)) else {
                continue;
            };
            if !sec.is_init_data() {
                continue;
            }
            out.push(
                Finding::new(
                    "SIZ-6",
                    format!("elf:{}:{}", sec.name, s.name),
                    format!(
                        "`{}` is {} of {}: its initial value is stored in flash and copied to RAM at startup",
                        s.name,
                        bytes(s.size),
                        sec.name
                    ),
                )
                .span(fw.symbol_span(s))
                .fix("make its initial value all zeros (it goes to .bss) and set it up in place at run time"),
            );
        }
        out
    }
}
