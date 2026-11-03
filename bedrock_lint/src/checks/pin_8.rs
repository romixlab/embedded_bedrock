//! `PIN-8` Unused pins left floating inputs

use crate::check::{Check, CheckMeta, Need, Registered};
use crate::finding::{Confidence, Finding};
use crate::model::Firmware;
use crate::model::scan;

pub struct Pin8;

static META: CheckMeta = CheckMeta {
    id: "PIN-8",
    needs: &[Need::Sources],
};

inventory::submit!(Registered(&Pin8));

/// STM32 families whose GPIOs come out of reset as floating inputs (GPIOx_MODER reset value 0 on most ports);
/// the newer ones (G0, G4, L0, L4, L5, H7, U5, WB, ...) reset to analog, where an unused pin is already fine
const FLOATING_AT_RESET: &[&str] = &[
    "stm32f0", "stm32f1", "stm32f2", "stm32f3", "stm32f4", "stm32f7", "stm32l1",
];

impl Check for Pin8 {
    fn meta(&self) -> &'static CheckMeta {
        &META
    }

    /// On an STM32 family whose pins reset to floating inputs: nothing in the code puts pins in analog mode
    /// (`set_as_analog`, `into_analog`, `Analog`) or names unused pins. An estimate: the board may tie every pin.
    fn run(&self, fw: &Firmware) -> Vec<Finding> {
        let Some(fam) = scan::family(fw) else {
            return Vec::new();
        };
        if !FLOATING_AT_RESET.contains(&fam.as_str()) {
            return Vec::new();
        }
        let idents = scan::code_idents(fw);
        let handled = idents.iter().any(|i| {
            let l = i.name.to_ascii_lowercase();
            l.contains("analog") || l.contains("unused")
        });
        if handled {
            return Vec::new();
        }
        vec![
            Finding::new(
                "PIN-8",
                "unused-pins",
                format!("{fam} pins come out of reset as floating inputs and nothing puts the unused ones in analog mode: they pick up noise and draw current"),
            )
            .span(scan::entry_span(fw))
            .fix("set every pin the board does not use to analog at boot (`Flex::new(p.PXn).set_as_analog()`), or give it a pull")
            .confidence(Confidence::Estimate),
        ]
    }
}
