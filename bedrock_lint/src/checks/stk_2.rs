//! `STK-2` `cargo:rustc-linker=` (or another unknown `cargo:` key) printed by build.rs

use crate::check::{Check, CheckMeta, Need, Registered};
use crate::finding::{Finding, Span};
use crate::model::Firmware;
use crate::model::build_script::KNOWN_KEYS;

pub struct Stk2;

static META: CheckMeta = CheckMeta {
    id: "STK-2",
    needs: &[Need::BuildScript],
};

inventory::submit!(Registered(&Stk2));

impl Check for Stk2 {
    fn meta(&self) -> &'static CheckMeta {
        &META
    }

    /// A one-colon `cargo:KEY=` with a key Cargo does not know is `links` metadata: without `package.links` Cargo
    /// drops it silently. `rustc-linker` is the usual one (meant to set flip-link). The two-colon form is not
    /// reported: Cargo itself fails on an unknown key there.
    fn run(&self, fw: &Firmware) -> Vec<Finding> {
        let Some(b) = &fw.build_script else {
            return Vec::new();
        };
        let links = fw.manifest.get(&["package", "links"]).is_some();
        let mut out = Vec::new();
        for d in &b.directives {
            if d.double_colon || KNOWN_KEYS.contains(&d.key.as_str()) {
                continue;
            }
            let span = Span::at(&b.file.rel, d.line, d.column);
            let item = format!("{}:cargo:{}={}", b.file.rel, d.key, d.value);
            if d.key == "rustc-linker" {
                out.push(
                    Finding::new(
                        "STK-2",
                        item,
                        format!(
                            "build.rs prints `cargo:rustc-linker={}`: no such build script instruction, Cargo ignores it and links with the default linker",
                            d.value
                        ),
                    )
                    .span(span)
                    .fix("remove the line and set `linker = \"flip-link\"` under `[target.<triple>]` in .cargo/config.toml (STK-1)"),
                );
            } else if !links {
                out.push(
                    Finding::new(
                        "STK-2",
                        item,
                        format!(
                            "build.rs prints `cargo:{}=...`: not a build script instruction, and without `package.links` Cargo drops it silently",
                            d.key
                        ),
                    )
                    .span(span)
                    .fix("use one of Cargo's instructions (`cargo::rustc-link-arg=`, ...); the two-colon `cargo::` form makes Cargo reject unknown keys"),
                );
            }
        }
        out
    }
}
