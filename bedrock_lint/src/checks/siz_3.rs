//! `SIZ-3` Per-package `opt-level` together with `lto = "fat"`: the override does nothing

use crate::check::{Check, CheckMeta, Registered};
use crate::finding::{Finding, Span};
use crate::model::{Firmware, TomlFile};

pub struct Siz3;

static META: CheckMeta = CheckMeta {
    id: "SIZ-3",
    needs: &[],
};

inventory::submit!(Registered(&Siz3));

fn is_fat(v: Option<&toml::Value>) -> bool {
    matches!(v, Some(toml::Value::String(s)) if s == "fat")
        || matches!(v, Some(toml::Value::Boolean(true)))
}

/// Where cargo reads `[profile]` from: the workspace root when the crate is a member of one, else its own manifest
fn profile_source(fw: &Firmware) -> &TomlFile {
    fw.workspace.as_ref().unwrap_or(&fw.manifest)
}

impl Check for Siz3 {
    fn meta(&self) -> &'static CheckMeta {
        &META
    }

    fn run(&self, fw: &Firmware) -> Vec<Finding> {
        let file = profile_source(fw);
        let Some(profiles) = file.table.get("profile").and_then(|v| v.as_table()) else {
            return Vec::new();
        };
        let mut out = Vec::new();
        for (name, prof) in profiles {
            let Some(prof_table) = prof.as_table() else {
                continue;
            };
            if !is_fat(prof_table.get("lto")) {
                continue;
            }
            let Some(packages) = prof_table.get("package").and_then(|v| v.as_table()) else {
                continue;
            };
            for (pkg, pv) in packages {
                if pv.get("opt-level").is_none() {
                    continue;
                }
                let path = [
                    "profile",
                    name.as_str(),
                    "package",
                    pkg.as_str(),
                    "opt-level",
                ];
                out.push(
                    Finding::new(
                        "SIZ-3",
                        format!("profile.{name}.package.{pkg}.opt-level"),
                        format!(
                            "profile.{name} sets lto = \"fat\" and overrides opt-level for package \"{pkg}\": \
                             fat LTO merges codegen units across the whole dependency graph, the per-package \
                             override does nothing"
                        ),
                    )
                    .span(Span::new(file.rel(), file.line_of(&path)))
                    .fix(format!(
                        "drop the opt-level override for \"{pkg}\", or use `lto = \"thin\"`/no lto if the \
                         override must take effect"
                    )),
                );
            }
        }
        out
    }
}
