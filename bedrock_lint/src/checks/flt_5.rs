//! `FLT-5` Reset cause not read and reported at boot

use crate::check::{Check, CheckMeta, Need, Registered};
use crate::finding::Finding;
use crate::model::Firmware;
use crate::model::scan;

pub struct Flt5;

static META: CheckMeta = CheckMeta {
    id: "FLT-5",
    needs: &[Need::Sources],
};

inventory::submit!(Registered(&Flt5));

/// Names that read a reset cause: the template's and bedrock_crash's, the reset flag registers of STM32 (RCC_CSR,
/// RCC_RSR), RP (CHIP_RESET, WATCHDOG.REASON), nRF (RESETREAS), ESP (reset_reason)
const READS: &[&str] = &[
    "reset_cause",
    "ResetCause",
    "reset_reason",
    "ResetReason",
    "ResetFlags",
    "reset_flags",
    "last_reset",
    "LastReset",
    "RCC_CSR",
    "RCC_RSR",
    "csr",
    "rsr",
    "CHIP_RESET",
    "chip_reset",
    "RESETREAS",
    "resetreas",
    "had_watchdog_reset_powman",
];

impl Check for Flt5 {
    fn meta(&self) -> &'static CheckMeta {
        &META
    }

    /// Nothing in the code names a reset cause or the register that holds it.
    fn run(&self, fw: &Firmware) -> Vec<Finding> {
        if scan::first_ident(&scan::code_idents(fw), READS).is_some() {
            return Vec::new();
        }
        vec![
            Finding::new(
                "FLT-5",
                "reset-cause",
                "the reset cause is never read: a brown-out, a watchdog or a lockup reset looks like a normal start",
            )
            .span(scan::entry_span(fw))
            .fix("read the reset flags at boot (RCC_CSR / RCC_RSR, RP CHIP_RESET and WATCHDOG.REASON), clear them, and report the cause (the template's last_reset.rs, `bedrock_crash::boot`)"),
        ]
    }
}
