//! What the FLT checks share: the core exception handlers of the source and the checks' wording

use crate::model::Firmware;
use crate::model::scan::{self, FnItem};

/// The handler of a core exception: `#[exception] fn NAME`, or `extern "C" fn NAME` (a handler placed by hand, as
/// the template's bootloader does with `.HardFault.user`)
pub fn handler<'a>(fns: &'a [FnItem<'a>], name: &str) -> Option<&'a FnItem<'a>> {
    fns.iter().find(|f| {
        f.name() == name && f.self_ty.is_none() && (f.has_attr("exception") || f.sig.abi.is_some())
    })
}

/// Logging, counting and panicking macros a fault handler must not run (FLT-2)
pub fn nesting_macro(name: &str) -> bool {
    matches!(
        name,
        "trace"
            | "debug"
            | "info"
            | "warn"
            | "error"
            | "println"
            | "unwrap"
            | "expect"
            | "panic"
            | "assert"
            | "assert_eq"
            | "assert_ne"
            | "unreachable"
            | "todo"
            | "unimplemented"
    ) || scan::COUNT_MACROS.contains(&name)
}

/// Whether the firmware's core has the configurable faults (UsageFault, MemManage, BusFault): Armv7-M and Armv8-M
/// Mainline, by target triple, else by chip family. `None` when neither tells.
pub fn has_configurable_faults(fw: &Firmware) -> Option<bool> {
    if let Some(core) = scan::core(fw) {
        return Some(!matches!(core, "M0+" | "M23"));
    }
    let fam = scan::family(fw)?;
    let m0 = [
        "stm32c0", "stm32f0", "stm32g0", "stm32l0", "stm32u0", "rp2040",
    ];
    if m0.contains(&fam.as_str()) {
        return Some(false);
    }
    Some(fam.starts_with("stm32") || fam == "rp235x" || fam.starts_with("nrf"))
}
