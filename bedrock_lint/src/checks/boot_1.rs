//! `BOOT-1` Firmware entered by a jump (ROM DfuSe leave, bootloader) not detected

use crate::check::{Check, CheckMeta, Need, Registered};
use crate::finding::Finding;
use crate::model::Firmware;
use crate::model::scan;

pub struct Boot1;

static META: CheckMeta = CheckMeta {
    id: "BOOT-1",
    needs: &[Need::Sources],
};

inventory::submit!(Registered(&Boot1));

/// Ways into the STM32 ROM bootloader by hand: its DfuSe leave then jumps back into this image without a reset
const ROM_ENTRY: &[&str] = &[
    "bootload",
    "jump_to_bootloader",
    "jump_to_system_memory",
    "jump_to_rom",
    "SYSTEM_MEMORY",
    "ROM_BOOTLOADER",
];

/// bedrock_rom_boot's functions that look at how the firmware was entered
const DETECT: &[&str] = &["early", "early_with", "entered"];

impl Check for Boot1 {
    fn meta(&self) -> &'static CheckMeta {
        &META
    }

    /// A firmware that can send itself into the STM32 ROM bootloader (`cortex_m::asm::bootload`, a jump to system
    /// memory, bedrock_rom_boot) comes back by the ROM's jump, not a reset, and has to see that: through
    /// bedrock_rom_boot's `early()` / `entered()`, or by reading VTOR or RCC_CFGR.SWS itself. RP, nRF and ESP ROMs
    /// reset on the way out, so they are left alone.
    fn run(&self, fw: &Firmware) -> Vec<Finding> {
        if scan::family(fw).is_some_and(|f| !f.starts_with("stm32")) {
            return Vec::new();
        }
        let words = scan::words(fw);
        let rom_boot = words.iter().find(|w| w.text == "bedrock_rom_boot");
        let by_hand = words.iter().find(|w| ROM_ENTRY.contains(&w.text.as_str()));
        let Some(way) = rom_boot.or(by_hand) else {
            return Vec::new();
        };
        let detected = (rom_boot.is_some()
            && words.iter().any(|w| DETECT.contains(&w.text.as_str())))
            || reads_entry_state(fw);
        if detected {
            return Vec::new();
        }
        let message = if rom_boot.is_some() {
            "the firmware uses bedrock_rom_boot but never calls its early(): the ROM's DfuSe leave jumps into the image with the ROM's PLL, SysTick, VTOR and USB interrupt still set"
        } else {
            "the firmware enters the ROM bootloader but doesn't detect the way back: the ROM's DfuSe leave jumps into the image with its PLL, SysTick, VTOR and USB interrupt still set"
        };
        vec![
            Finding::new("BOOT-1", way.text.clone(), message)
                .span(way.loc.span().label("way into the ROM bootloader"))
                .fix("call bedrock_rom_boot::early() first in main (it checks RCC_CFGR.SWS, VTOR and the NVIC and resets once when the ROM jumped in)"),
        ]
    }
}

/// Whether some function reads VTOR or RCC_CFGR.SWS: the firmware looks at how it was entered itself
fn reads_entry_state(fw: &Firmware) -> bool {
    scan::functions(fw).iter().any(|f| {
        let calls = scan::method_calls(f);
        if calls.iter().any(|c| c.name == "sws") {
            return true;
        }
        let reads = calls.iter().any(|c| c.name == "read")
            || scan::calls_in(f)
                .iter()
                .any(|(n, _)| n.ends_with("read_volatile") || n.ends_with("read"));
        reads && scan::words_in(f).iter().any(|w| w.has("vtor"))
    })
}
