//! `FLS-2` More than one owner of the flash peripheral

use super::fls_1::flash_drivers;
use crate::check::{Check, CheckMeta, Need, Registered};
use crate::finding::Finding;
use crate::model::Firmware;
use crate::model::scan;

pub struct Fls2;

static META: CheckMeta = CheckMeta {
    id: "FLS-2",
    needs: &[Need::Sources],
};

inventory::submit!(Registered(&Fls2));

impl Check for Fls2 {
    fn meta(&self) -> &'static CheckMeta {
        &META
    }

    /// More than one flash driver built (`Flash::new*` twice: the updater and the settings store each take
    /// `FLASH`, one through `steal()`), or the `FLASH` singleton stolen: two drivers interleave erase and program
    /// sequences on one controller. One driver behind a blocking mutex, shared, is the fix (FEATURES TPL-24).
    fn run(&self, fw: &Firmware) -> Vec<Finding> {
        let drivers = flash_drivers(fw);
        let mut out = Vec::new();
        if let [(_, first), rest @ ..] = drivers.as_slice() {
            for (path, at) in rest {
                out.push(
                    Finding::new(
                        "FLS-2",
                        path.clone(),
                        "a second flash driver: two owners of the flash controller interleave their erase and program sequences",
                    )
                    .span(at.span().label("second driver"))
                    .span(first.span().label("first driver"))
                    .fix("build one Flash, put it in a blocking Mutex<RefCell<..>> and share it (as FirmwareUpdaterConfig::from_linkerfile_blocking does)"),
                );
            }
        }
        for f in scan::functions(fw) {
            scan::each(f.block, |n| {
                if let scan::Node::Expr(e @ syn::Expr::Call(_)) = n
                    && let Some(name) = scan::call_name(e)
                    && name.ends_with("FLASH::steal")
                {
                    out.push(
                        Finding::new(
                            "FLS-2",
                            format!("{}:steal", f.path()),
                            "the FLASH peripheral is stolen: whoever owns it already gets a second owner",
                        )
                        .span(scan::loc(f.file, scan::head_span(e)).span())
                        .fix("pass the one shared flash driver instead of stealing the peripheral"),
                    );
                }
            });
        }
        out
    }
}
