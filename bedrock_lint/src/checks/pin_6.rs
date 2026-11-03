//! `PIN-6` Two owners of a pin

use super::pin;
use crate::check::{Check, CheckMeta, Need, Registered};
use crate::finding::{Confidence, Finding};
use crate::model::Firmware;
use crate::model::scan;
use std::collections::BTreeMap;

pub struct Pin6;

static META: CheckMeta = CheckMeta {
    id: "PIN-6",
    needs: &[Need::Sources],
};

inventory::submit!(Registered(&Pin6));

impl Check for Pin6 {
    fn meta(&self) -> &'static CheckMeta {
        &META
    }

    /// The compiler stops a pin moved twice, so a second owner comes in through `steal()` (`PA5::steal()`,
    /// `Peripherals::steal().PA5`) or `clone_unchecked()`. A pin taken that way that the code also names elsewhere is
    /// an estimate: a diagnostic API and a driver task may both drive it (usb_io_fw IF-16). Proving the two never run
    /// at once needs a person.
    fn run(&self, fw: &Firmware) -> Vec<Finding> {
        let idents = scan::code_idents(fw);
        let mut uses: BTreeMap<&str, Vec<usize>> = BTreeMap::new();
        for (i, id) in idents.iter().enumerate() {
            if pin::is_pin_name(&id.name) {
                uses.entry(id.name.as_str()).or_default().push(i);
            }
        }
        let mut out = Vec::new();
        for (name, at) in uses {
            // `PA5::steal()`, `steal().PA5`, `PA5.clone_unchecked()`: a steal within two identifiers
            let stolen: Vec<usize> = at
                .iter()
                .copied()
                .filter(|&i| {
                    let near = |j: usize| {
                        idents
                            .get(j)
                            .is_some_and(|x| x.name == "steal" || x.name == "clone_unchecked")
                    };
                    near(i + 1) || near(i.wrapping_sub(1)) || near(i + 2)
                })
                .collect();
            if stolen.is_empty() || at.len() < 2 {
                continue;
            }
            let s = &idents[stolen[0]];
            let mut f = Finding::new(
                "PIN-6",
                name.to_string(),
                format!("{name} is taken with `steal()` and also named elsewhere: two owners can drive it at once"),
            )
            .span(s.loc.span().label("stolen here"))
            .fix("give each pin one owner; a second user asks the owner (a channel, a claim API that refuses while it is in use)")
            .confidence(Confidence::Estimate);
            for &o in at.iter().filter(|i| !stolen.contains(i)).take(3) {
                f = f.span(idents[o].loc.span().label("also here"));
            }
            out.push(f);
        }
        out
    }
}
