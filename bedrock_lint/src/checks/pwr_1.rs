//! `PWR-1` No brown-out reset level or programmable voltage detector (PVD) for the supply the board really has

use crate::check::{Check, CheckMeta, Need, Registered};
use crate::finding::Finding;
use crate::model::Firmware;
use crate::model::scan;

pub struct Pwr1;

static META: CheckMeta = CheckMeta {
    id: "PWR-1",
    needs: &[Need::Sources],
};

inventory::submit!(Registered(&Pwr1));

/// Words of the PVD (`set_pvde`, `pls`, `PVD_PVM`) and the BOR level (`BOR_LEV`, `borr_lev`, `borf_lev`); not
/// `brownout`, which reset-cause code names without setting anything
const SUPPLY_WATCH: &[&str] = &[
    "pvd", "pvde", "pvdo", "pls", "pvm", "bor", "borr", "borf", "borlev",
];

impl Check for Pwr1 {
    fn meta(&self) -> &'static CheckMeta {
        &META
    }

    /// An STM32 firmware whose source names neither the PVD nor a BOR level: below the POR/PDR threshold it runs
    /// on a supply too low for its clock and flash (the default BOR is off on G0, the lowest level on F4 / H7). A
    /// BOR level set in the option bytes at production is a waiver with that reason.
    fn run(&self, fw: &Firmware) -> Vec<Finding> {
        if !scan::is_stm32(fw) {
            return Vec::new();
        }
        let words = scan::words(fw);
        if SUPPLY_WATCH.iter().any(|s| scan::mentions(&words, s)) {
            return Vec::new();
        }
        let at = scan::entry_loc(fw);
        vec![
            Finding::new(
                "PWR-1",
                "supply",
                "no brown-out reset level and no PVD: a sagging supply runs the chip below what its clock and flash need instead of holding it in reset",
            )
            .span(at.span())
            .fix("set the BOR level in the option bytes for the board's supply, or enable the PVD (PWR_CR2.PVDE, PLS) with its interrupt and count it"),
        ]
    }
}
