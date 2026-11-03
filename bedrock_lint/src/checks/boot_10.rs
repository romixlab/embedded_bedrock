//! `BOOT-10` Backup-domain helper that turns the RTC clock or `PWR_CR1.DBP` off afterwards instead of restoring
//! what it found

use crate::check::{Check, CheckMeta, Need, Registered};
use crate::finding::Finding;
use crate::model::Firmware;
use crate::model::scan;

pub struct Boot10;

static META: CheckMeta = CheckMeta {
    id: "BOOT-10",
    needs: &[Need::Sources],
};

inventory::submit!(Registered(&Boot10));

/// The bits a helper turns on to reach the backup domain
const BITS: &[&str] = &["set_dbp", "set_rtcen", "set_rtcapben"];

impl Check for Boot10 {
    fn meta(&self) -> &'static CheckMeta {
        &META
    }

    /// In one function, `set_dbp(true)` (or `set_rtcen(true)`) and later the same setter with a literal `false`:
    /// whoever had it on before the call (the RTC, another helper, the ROM's boot word) finds it off. A restore of a
    /// saved value (`set_dbp(dbp)`) is right; a function that resets the domain (`set_bdrst`) owns it and may leave
    /// DBP off.
    fn run(&self, fw: &Firmware) -> Vec<Finding> {
        let mut out = Vec::new();
        for f in scan::functions(fw) {
            let calls = scan::method_calls(&f);
            if calls.iter().any(|c| c.name == "set_bdrst") {
                continue;
            }
            for bit in BITS {
                let value = |c: &scan::MethodCall| {
                    (c.name == *bit && c.args.len() == 1)
                        .then(|| scan::bool_value(&c.args[0]))
                        .flatten()
                };
                let Some(on) = calls.iter().find(|c| value(c) == Some(true)) else {
                    continue;
                };
                let Some(off) = calls
                    .iter()
                    .find(|c| c.loc > on.loc && value(c) == Some(false))
                else {
                    continue;
                };
                let reg = bit.trim_start_matches("set_").to_ascii_uppercase();
                out.push(
                    Finding::new(
                        "BOOT-10",
                        format!("{}:{reg}", f.path()),
                        format!(
                            "{} turns {reg} on and then off: whoever had it on before the call finds it off (the RTC stops, backup-register writes are lost)",
                            f.name()
                        ),
                    )
                    .span(off.loc.span().label(format!("{reg} off")))
                    .span(on.loc.span().label(format!("{reg} on")))
                    .fix(format!(
                        "read {reg} first and restore that value (`let was = ..; ..; w.{bit}(was)`)"
                    )),
                );
            }
        }
        out
    }
}
