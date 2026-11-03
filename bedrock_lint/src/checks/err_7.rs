//! `ERR-7` RP2350 ROM reboot with delay 0 or FRCE_OFF bits beyond PROC1 (RP2350-E30, E19)

use crate::check::{Check, CheckMeta, Need, Registered};
use crate::finding::Finding;
use crate::model::Firmware;
use crate::model::scan::{self, Node};
use syn::Expr;

pub struct Err7;

static META: CheckMeta = CheckMeta {
    id: "ERR-7",
    needs: &[Need::Sources],
};

inventory::submit!(Registered(&Err7));

impl Check for Err7 {
    fn meta(&self) -> &'static CheckMeta {
        &META
    }

    /// On RP2350:
    /// - the bootrom's `reboot(flags, delay_ms, p0, p1)` (`rom_data::reboot`) with a literal delay of 0. RP2350
    ///   datasheet D.2.13, RP2350-E30 "reboot() API fails to reboot for delay_ms == 0", affects A2, A3, A4,
    ///   workaround "Pass delay_ms = 1 instead."
    /// - a write of `FRCE_OFF` that sets a bit other than PROC1. D.2.6, RP2350-E19 "RP2350 reboot hangs if certain
    ///   bits are set in FRCE_OFF when rebooting", affects A2 (fixed by the A3 bootrom), workaround "Don't perform a
    ///   WATCHDOG or POWMAN boot, or a core0 reset with bits other than FRCE_OFF.PROC1 set."
    fn run(&self, fw: &Firmware) -> Vec<Finding> {
        if !scan::family_in(fw, &["rp235x"]) {
            return Vec::new();
        }
        let consts = scan::consts(fw);
        let mut out = Vec::new();
        for f in scan::functions(fw) {
            scan::each(f.block, |n| match n {
                Node::Expr(e @ Expr::Call(c)) => {
                    let Some(name) = scan::call_name(e) else {
                        return;
                    };
                    let rom = name.rsplit("::").next() == Some("reboot")
                        && (name.contains("rom") || c.args.len() == 4);
                    if rom && c.args.len() == 4 && scan::int_value(&c.args[1], &consts) == Some(0) {
                        out.push(
                            Finding::new(
                                "ERR-7",
                                format!("{}:reboot-delay", f.path()),
                                "bootrom reboot() with delay_ms 0: RP2350-E30 \"reboot() API fails to reboot for delay_ms == 0\" (A2, A3, A4), the chip doesn't reboot",
                            )
                            .span(scan::loc(f.file, scan::head_span(e)).span())
                            .fix("pass delay_ms = 1 (the erratum's workaround)"),
                        );
                    }
                }
                Node::Expr(Expr::MethodCall(m))
                    if matches!(m.method.to_string().as_str(), "modify" | "write")
                        && scan::text(&*m.receiver).contains("frce_off") =>
                {
                    for a in &m.args {
                        scan::each_in(a, |n| {
                            if let Node::Expr(Expr::MethodCall(s)) = n {
                                let bit = s.method.to_string();
                                if bit.starts_with("set_")
                                    && bit != "set_proc1"
                                    && s.args.first().and_then(scan::bool_value) == Some(true)
                                {
                                    out.push(
                                        Finding::new(
                                            "ERR-7",
                                            format!("{}:frce_off", f.path()),
                                            format!(
                                                "FRCE_OFF bit {} set: RP2350-E19 \"RP2350 reboot hangs if certain bits are set in FRCE_OFF when rebooting\" (A2), a watchdog, POWMAN or core 0 reset with it set hangs in the bootrom",
                                                bit.trim_start_matches("set_").to_ascii_uppercase()
                                            ),
                                        )
                                        .span(scan::loc(f.file, s.method.span()).span())
                                        .fix("clear every FRCE_OFF bit but PROC1 before a reboot (the erratum's workaround)"),
                                    );
                                }
                            }
                        });
                    }
                }
                _ => {}
            });
        }
        out
    }
}
