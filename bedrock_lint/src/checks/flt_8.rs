//! `FLT-8` No boot-loop rescue

use crate::check::{Check, CheckMeta, Need, Registered};
use crate::finding::Finding;
use crate::model::Firmware;
use crate::model::scan;

pub struct Flt8;

static META: CheckMeta = CheckMeta {
    id: "FLT-8",
    needs: &[Need::Sources],
};

inventory::submit!(Registered(&Flt8));

/// A boot-loop rescue: bedrock_rom_boot's (`early()`, `Start::Rescue`), a count of its own, or embassy-boot's
/// rollback of an image that was never marked booted
const RESCUES: &[&str] = &[
    "bedrock_rom_boot",
    "Rescue",
    "BOOT_LOOP_LIMIT",
    "boot_loop",
    "unhealthy_boots",
    "mark_booted",
];

impl Check for Flt8 {
    fn meta(&self) -> &'static CheckMeta {
        &META
    }

    /// A Cortex-M firmware whose code names none of the rescues: a crash at start-up repeats on every boot and
    /// the board can only be recovered with a probe.
    fn run(&self, fw: &Firmware) -> Vec<Finding> {
        if !scan::cortex_m(fw) || scan::first_ident(&scan::code_idents(fw), RESCUES).is_some() {
            return Vec::new();
        }
        vec![
            Finding::new(
                "FLT-8",
                "boot-loop",
                "no boot-loop rescue: a crash at start-up repeats on every boot, and only a probe gets the board back",
            )
            .span(scan::entry_span(fw))
            .fix("count boots without a healthy mark in a backup register and fall back to the ROM bootloader after a few (bedrock_rom_boot::early, the template's boot.rs)"),
        ]
    }
}
