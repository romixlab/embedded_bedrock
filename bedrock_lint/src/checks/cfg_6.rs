//! `CFG-6` thumbv6m without `portable-atomic` + `critical-section` where a crate needs CAS (`static_cell`)

use crate::check::{Check, CheckMeta, Registered};
use crate::finding::{Finding, Span};
use crate::model::Firmware;

pub struct Cfg6;

static META: CheckMeta = CheckMeta {
    id: "CFG-6",
    needs: &[],
};

inventory::submit!(Registered(&Cfg6));

const FIX: &str = "add `portable-atomic = { version = \"*\", features = [\"critical-section\"] }`: thumbv6m \
     (Cortex-M0/M0+) has no CAS instruction, static_cell needs one; its critical-section feature pulls \
     the critical-section crate in on its own, no separate dependency on it is needed";

impl Check for Cfg6 {
    fn meta(&self) -> &'static CheckMeta {
        &META
    }

    /// `critical-section` itself doesn't need to be a direct dependency: portable-atomic's `critical-section`
    /// feature pulls it in
    fn run(&self, fw: &Firmware) -> Vec<Finding> {
        let Some((_, triple)) = fw.build_target() else {
            return Vec::new();
        };
        if !triple.starts_with("thumbv6m") {
            return Vec::new();
        }
        let deps = fw.dependencies();
        if !deps.iter().any(|d| d.package() == "static_cell") {
            return Vec::new();
        }
        let portable_atomic = deps.iter().find(|d| d.package() == "portable-atomic");
        let has_cs_feature = portable_atomic
            .is_some_and(|d| d.features().iter().any(|(f, _, _)| f == "critical-section"));
        if has_cs_feature {
            return Vec::new();
        }
        let (span, what) = match portable_atomic {
            None => (
                Span::new(fw.manifest.rel(), 0),
                "no portable-atomic dependency".to_string(),
            ),
            Some(d) => {
                let mut path: Vec<&str> = d.table.iter().map(String::as_str).collect();
                path.push(&d.name);
                (
                    Span::new(d.file.rel(), d.file.line_of(&path)),
                    "portable-atomic without its critical-section feature".to_string(),
                )
            }
        };
        vec![
            Finding::new(
                "CFG-6",
                "portable-atomic",
                format!("{triple} has no CAS instruction and static_cell needs one: {what}"),
            )
            .span(span)
            .fix(FIX),
        ]
    }
}
