//! The PCB side of the checks: which net each MCU pin is on.

use std::collections::BTreeMap;

/// The MCU's pins on the PCB, as the caller read them from the board file (`bedrock_board::geometry`).
#[derive(Debug, Clone, Default)]
pub struct McuPins {
    /// The MCU's reference (`U201`).
    pub reference: String,
    /// GPIO name (`PC14`, not the symbol's `PC14-OSC32_IN`) to its net; `None` when nothing else is on the net
    /// (KiCad's `unconnected-...`).
    pub nets: BTreeMap<String, Option<String>>,
}

impl McuPins {
    /// The net of `pin`: `None` when the PCB has no such pad, `Some(None)` when the pad is not connected.
    pub fn net(&self, pin: &str) -> Option<Option<&str>> {
        self.nets.get(pin).map(Option::as_deref)
    }
}

/// Whether the description's `net = "..."` names the PCB's net: the whole name, or its last path segment
/// (`LED_R` is `/MCU_H7/LED_R`).
pub(crate) fn net_matches(pcb: &str, described: &str) -> bool {
    pcb == described || pcb.rsplit('/').next() == Some(described)
}
