//! `PWR-4` RTC clock source changed in the clock config of a firmware that keeps state in the backup domain

use crate::check::{Check, CheckMeta, Need, Registered};
use crate::finding::{Confidence, Finding};
use crate::model::Firmware;
use crate::model::scan::{self, Node};
use syn::Expr;

pub struct Pwr4;

static META: CheckMeta = CheckMeta {
    id: "PWR-4",
    needs: &[Need::Sources],
};

inventory::submit!(Registered(&Pwr4));

/// Families where embassy-stm32 0.6 resets the backup domain when RCC_BDCR differs from `config.rcc.ls` (`rcc/bd.rs`
/// leaves it alone on L0, L1, H5, H7RS, C0 and N6)
fn resets_domain(family: &str) -> bool {
    family.starts_with("stm32")
        && !matches!(
            family,
            "stm32l0" | "stm32l1" | "stm32h5" | "stm32c0" | "stm32n6"
        )
}

/// Setters of RCC_BDCR's clock fields that embassy compares at the next boot
const BDCR_SETTERS: &[&str] = &[
    "set_rtcsel",
    "set_rtcen",
    "set_lseon",
    "set_lsebyp",
    "set_lsedrv",
];

impl Check for Pwr4 {
    fn meta(&self) -> &'static CheckMeta {
        &META
    }

    /// embassy-stm32's `init` compares RTCSEL, RTCEN and the LSE bits of RCC_BDCR with `config.rcc.ls` and pulses
    /// BDRST when they differ, which clears the backup registers. In a firmware that keeps state there (backup
    /// registers, `bkp_cnt!`, bedrock_rom_boot's boot word), a write of those bits by hand is certain to cost it at
    /// the next boot; an RTC source other than embassy's default LSI costs it once whenever a firmware with another
    /// source ran before (an estimate).
    fn run(&self, fw: &Firmware) -> Vec<Finding> {
        if !scan::family(fw).is_some_and(|f| resets_domain(&f))
            || !scan::depends_on(fw, "embassy-stm32")
        {
            return Vec::new();
        }
        let words = scan::words(fw);
        let keeps = words.iter().any(|w| {
            w.text == "bedrock_rom_boot"
                || w.has("bkp")
                || w.has("bkpr")
                || w.has("tamp")
                || w.has("backup")
        });
        if !keeps {
            return Vec::new();
        }
        let mut out = Vec::new();
        for f in scan::functions(fw) {
            for c in scan::method_calls(&f) {
                if BDCR_SETTERS.contains(&c.name.as_str()) {
                    out.push(
                        Finding::new(
                            "PWR-4",
                            format!("{}:{}", f.path(), c.name),
                            format!(
                                "RCC_BDCR written by hand ({}): embassy compares it with config.rcc.ls at the next boot and resets the backup domain, losing the state kept there",
                                c.name
                            ),
                        )
                        .span(c.loc.span())
                        .fix("set the RTC clock through config.rcc.ls only, and keep it the same in every firmware of the board"),
                    );
                }
            }
            scan::each(f.block, |n| {
                let Node::Expr(e) = n else { return };
                let Some(choice) = rtc_choice(e) else { return };
                out.push(
                    Finding::new(
                        "PWR-4",
                        format!("{}:{choice}", f.path()),
                        format!(
                            "the RTC clock is set to {choice}, not embassy's default LSI: whenever a firmware (or a bootloader) with another source ran before, embassy resets the backup domain and the state kept there is gone"
                        ),
                    )
                    .span(scan::loc(f.file, syn::spanned::Spanned::span(e)).span())
                    .fix("keep the LSI, or use the same RTC source in every image of the board (bootloader included) and accept one reset when it changes")
                    .confidence(Confidence::Estimate),
                );
            });
        }
        out
    }
}

/// `RtcClockSource::LSE`, `LsConfig::default_lse()`, `LsConfig::off()`: an RTC source other than the LSI
fn rtc_choice(e: &Expr) -> Option<String> {
    // a call's function is a path of its own: `LsConfig::default_lse()` is seen once, as its path
    let Expr::Path(p) = e else { return None };
    let path = &p.path;
    let segs: Vec<String> = path.segments.iter().map(|s| s.ident.to_string()).collect();
    let [.., ty, last] = segs.as_slice() else {
        return None;
    };
    match ty.as_str() {
        "RtcClockSource" if last != "LSI" => Some(format!("{ty}::{last}")),
        "LsConfig" if !matches!(last.as_str(), "default_lsi" | "new" | "default") => {
            Some(format!("{ty}::{last}"))
        }
        _ => None,
    }
}
