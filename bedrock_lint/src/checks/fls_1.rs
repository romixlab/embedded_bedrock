//! `FLS-1` Flash that a power cut can tear read raw on an ECC part

use crate::check::{Check, CheckMeta, Need, Registered};
use crate::finding::Finding;
use crate::model::Firmware;
use crate::model::scan;

pub struct Fls1;

static META: CheckMeta = CheckMeta {
    id: "FLS-1",
    needs: &[Need::Sources],
};

inventory::submit!(Registered(&Fls1));

/// STM32 families whose flash has ECC (a double error is an NMI or a bus fault); FEATURES TPL-26 lists the others
const ECC_FLASH: &[&str] = &[
    "stm32g0", "stm32h7", "stm32g4", "stm32l4", "stm32l5", "stm32u5", "stm32wb", "stm32wl",
    "stm32h5", "stm32c0", "stm32u0",
];

impl Check for Fls1 {
    fn meta(&self) -> &'static CheckMeta {
        &META
    }

    /// A flash driver built (`Flash::new`, `Flash::new_blocking`: settings, the embassy-boot state, a log) on an ECC
    /// part with no `ww_ecc_flash` in the firmware: a word half-programmed by a power cut has a double ECC error,
    /// and reading it raw is an NMI (G0) or a BusFault (H7) on every boot. On G0 the wrapper also needs its hook,
    /// `ww_ecc_flash::on_nmi()`, in the NMI handler.
    fn run(&self, fw: &Firmware) -> Vec<Finding> {
        if !scan::family_in(fw, ECC_FLASH) {
            return Vec::new();
        }
        let words = scan::words(fw);
        let wrapper = words.iter().find(|w| {
            matches!(
                w.text.as_str(),
                "ww_ecc_flash" | "EccSafeFlash" | "EccStrictFlash"
            )
        });
        let mut out = Vec::new();
        match wrapper {
            None => {
                for (path, at) in flash_drivers(fw) {
                    out.push(
                        Finding::new(
                            "FLS-1",
                            path,
                            "flash driver without ww_ecc_flash on an ECC part: a word torn by a power cut has a double ECC error, and reading it raw is an NMI (G0) or a BusFault (H7) on every boot",
                        )
                        .span(at.span())
                        .fix("wrap the driver in ww_ecc_flash::EccSafeFlash (and call ww_ecc_flash::on_nmi() first in the NMI handler on G0)"),
                    );
                }
            }
            Some(w)
                if scan::family_in(fw, &["stm32g0"])
                    && !words.iter().any(|w| w.text == "on_nmi") =>
            {
                out.push(
                    Finding::new(
                        "FLS-1",
                        "on_nmi",
                        "ww_ecc_flash without its NMI hook: on G0 a double ECC error is an NMI, and without ww_ecc_flash::on_nmi() in the handler the read doesn't come back as an error",
                    )
                    .span(w.loc.span())
                    .fix("#[exception] fn NonMaskableInt() { if ww_ecc_flash::on_nmi() { return; } .. }"),
                );
            }
            Some(_) => {}
        }
        out
    }
}

/// Flash driver constructors: `Flash::new`, `Flash::new_blocking`, `Flash::<..>::new_blocking`, with the function
/// and the place
pub(crate) fn flash_drivers(fw: &Firmware) -> Vec<(String, crate::model::Loc)> {
    let mut out = Vec::new();
    for f in scan::functions(fw) {
        scan::each(f.block, |n| {
            if let scan::Node::Expr(e @ syn::Expr::Call(_)) = n
                && let Some(name) = scan::call_name(e)
            {
                let mut segs = name.rsplit("::");
                let ctor = segs.next().unwrap_or_default();
                if segs.next() == Some("Flash") && ctor.starts_with("new") {
                    out.push((f.path(), scan::loc(f.file, scan::head_span(e))));
                }
            }
        });
    }
    out
}
