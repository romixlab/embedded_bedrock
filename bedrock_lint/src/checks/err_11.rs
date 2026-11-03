//! `ERR-11` Silicon revision read from a register that can't tell all revisions apart

use crate::check::{Check, CheckMeta, Need, Registered};
use crate::finding::Finding;
use crate::model::Firmware;
use crate::model::scan;

pub struct Err11;

static META: CheckMeta = CheckMeta {
    id: "ERR-11",
    needs: &[Need::Sources],
};

inventory::submit!(Registered(&Err11));

impl Check for Err11 {
    fn meta(&self) -> &'static CheckMeta {
        &META
    }

    /// On RP2350, `CHIP_ID.REVISION` read (`chip_id().read().revision()`) in a firmware that never reads the
    /// bootrom's version byte: RP2350 datasheet appendix C, "Stepping A3 is identified by a CHIP_ID.REVISION value
    /// of 0x3, and a bootrom version byte at offset 0x00000013 of 0x3", "Stepping A4 is identified by a
    /// CHIP_ID.REVISION value of 0x3, and a bootrom version byte at offset 0x00000013 of 0x4".
    fn run(&self, fw: &Firmware) -> Vec<Finding> {
        if !scan::family_in(fw, &["rp235x"]) {
            return Vec::new();
        }
        let words = scan::words(fw);
        let rom_version = words.iter().any(|w| {
            let s = scan::segments(&w.text);
            s.iter().any(|x| x == "bootrom" || x == "chip_version")
                || (s.iter().any(|x| x == "rom") && s.iter().any(|x| x == "version"))
        });
        if rom_version {
            return Vec::new();
        }
        let mut out = Vec::new();
        for f in scan::functions(fw) {
            for c in scan::method_calls(&f) {
                if c.name == "revision" && c.receiver.contains("chip_id") {
                    out.push(
                        Finding::new(
                            "ERR-11",
                            format!("{}:revision", f.path()),
                            "the silicon revision is taken from CHIP_ID.REVISION, which is 3 on both RP2350 A3 and A4",
                        )
                        .span(c.loc.span())
                        .fix("tell the revision by the bootrom version byte at 0x00000013 (3 on A3, 4 on A4), keep REVISION as the raw value"),
                    );
                }
            }
        }
        out
    }
}
