//! `SIZ-4` Release profile without `codegen-units = 1`, `lto`, `panic = "abort"`

use crate::check::{Check, CheckMeta, Registered};
use crate::finding::{Confidence, Finding, Span};
use crate::model::{Firmware, TomlFile};

pub struct Siz4;

static META: CheckMeta = CheckMeta {
    id: "SIZ-4",
    needs: &[],
};

inventory::submit!(Registered(&Siz4));

fn profile_source(fw: &Firmware) -> &TomlFile {
    fw.workspace.as_ref().unwrap_or(&fw.manifest)
}

fn has_lto(v: Option<&toml::Value>) -> bool {
    match v {
        Some(toml::Value::String(s)) => s != "off",
        Some(toml::Value::Boolean(b)) => *b,
        _ => false,
    }
}

impl Check for Siz4 {
    fn meta(&self) -> &'static CheckMeta {
        &META
    }

    /// `opt-level` is left for a human to choose by measurement; only presence of the other three is checked
    fn run(&self, fw: &Firmware) -> Vec<Finding> {
        let file = profile_source(fw);
        let release = file
            .table
            .get("profile")
            .and_then(|p| p.get("release"))
            .and_then(|v| v.as_table());
        let mut missing = Vec::new();
        if !release.is_some_and(|p| p.get("codegen-units").and_then(|v| v.as_integer()) == Some(1))
        {
            missing.push("codegen-units = 1");
        }
        if !release.is_some_and(|p| has_lto(p.get("lto"))) {
            missing.push("lto");
        }
        if !release.is_some_and(|p| p.get("panic").and_then(|v| v.as_str()) == Some("abort")) {
            missing.push("panic = \"abort\"");
        }
        if missing.is_empty() {
            return Vec::new();
        }
        let span = if release.is_some() {
            Span::new(file.rel(), file.line_of_table(&["profile", "release"]))
        } else {
            Span::new(file.rel(), 0)
        };
        vec![
            Finding::new(
                "SIZ-4",
                "profile.release",
                format!(
                    "release profile is missing {}: opt-level alone is the smallest lever (z vs 3 is 10-25 % \
                     speed, measure before choosing)",
                    missing.join(", ")
                ),
            )
            .span(span)
            .fix(format!("add {} to [profile.release]", missing.join(", ")))
            .confidence(Confidence::Estimate),
        ]
    }
}
