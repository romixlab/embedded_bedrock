//! `IRQ-4` Several drivers on one shared NVIC vector

use crate::check::{Check, CheckMeta, Need, Registered};
use crate::finding::{Confidence, Finding};
use crate::model::Firmware;

pub struct Irq4;

static META: CheckMeta = CheckMeta {
    id: "IRQ-4",
    needs: &[Need::Sources],
};

inventory::submit!(Registered(&Irq4));

/// A chip family's vectors known to carry more than one peripheral, for the finding's message; the check itself
/// needs no chip data, it only reads `bind_interrupts!` lines
const KNOWN_SHARED: &[(&str, &str, &str)] = &[(
    "stm32g0",
    "USB_UCPD1_2",
    "typec_dongle API-30: a driver that disables \"its\" interrupt on drop left the other one on this vector deaf",
)];

impl Check for Irq4 {
    fn meta(&self) -> &'static CheckMeta {
        &META
    }

    /// Every `bind_interrupts!` line with more than one handler bound to its vector: a driver that disables "its"
    /// interrupt (on drop, on an error path) leaves every other handler on the same vector unreachable too. An
    /// estimate when the handlers come from different drivers (`usb::`, `ucpd::`), a candidate when they are all one
    /// driver's (`dma::InterruptHandler<DMA1_CH2>, dma::InterruptHandler<DMA1_CH3>`), which shares the vector by
    /// design
    fn run(&self, fw: &Firmware) -> Vec<Finding> {
        let chip = fw.chip();
        let mut out = Vec::new();
        for b in &fw.facts.bindings {
            let n = handler_count(&b.handlers);
            if n <= 1 {
                continue;
            }
            let one_driver = driver_modules(&b.handlers).len() == 1;
            let mut message = format!(
                "{n} handlers are bound to the one NVIC vector {}: a driver that disables \"its\" interrupt \
                 (on drop, on an error path) leaves the others on it deaf too",
                b.vector
            );
            if let Some(note) = chip.and_then(|c| {
                KNOWN_SHARED
                    .iter()
                    .find(|(family, vector, _)| *family == c.family && *vector == b.vector)
            }) {
                message = format!("{message} ({})", note.2);
            }
            out.push(
                Finding::new("IRQ-4", format!("{}:{}", b.group, b.vector), message)
                    .span(
                        b.loc
                            .span()
                            .label(format!("{} => {}", b.vector, b.handlers)),
                    )
                    .confidence(if one_driver {
                        Confidence::Candidate
                    } else {
                        Confidence::Estimate
                    }),
            );
        }
        out
    }
}

/// The driver modules of a line's handlers: the path before `InterruptHandler` (`dma`, `embassy_stm32::usb`)
fn driver_modules(handlers: &str) -> std::collections::BTreeSet<String> {
    handlers
        .split("InterruptHandler")
        .filter_map(|before| {
            let path = before.trim_end().strip_suffix("::")?;
            let start = path
                .rfind(|c: char| !(c.is_ascii_alphanumeric() || c == '_' || c == ':' || c == ' '))
                .map_or(0, |i| i + 1);
            Some(path[start..].replace(' ', ""))
        })
        .collect()
}

/// Handlers of one `bind_interrupts!` line, comma-separated outside `<..>` generics
fn handler_count(handlers: &str) -> usize {
    let t = handlers.trim();
    if t.is_empty() {
        return 0;
    }
    let mut depth = 0i32;
    let mut count = 1usize;
    for c in t.chars() {
        match c {
            '<' => depth += 1,
            '>' => depth -= 1,
            ',' if depth <= 0 => count += 1,
            _ => {}
        }
    }
    count
}

#[cfg(test)]
mod tests {
    use super::{driver_modules, handler_count};

    #[test]
    fn counts_outside_generics() {
        assert_eq!(handler_count(""), 0);
        assert_eq!(handler_count("usb :: InterruptHandler < USB >"), 1);
        assert_eq!(
            handler_count("usb :: InterruptHandler < USB > , ucpd :: InterruptHandler < UCPD1 >"),
            2
        );
        assert_eq!(
            driver_modules(
                "dma :: InterruptHandler < DMA1_CH2 > , dma :: InterruptHandler < DMA1_CH3 >"
            )
            .len(),
            1
        );
        assert_eq!(
            driver_modules("usb :: InterruptHandler < USB > , ucpd :: InterruptHandler < UCPD1 >")
                .len(),
            2
        );
    }
}
