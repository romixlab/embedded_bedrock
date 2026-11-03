//! `MEM-12` defmt in the dependencies without `-Tdefmt.x` (the cfg half; the `.defmt` section itself is elf, LINT-5)

use crate::check::{Check, CheckMeta, Registered};
use crate::finding::{Confidence, Finding, Span};
use crate::model::Firmware;

pub struct Mem12;

static META: CheckMeta = CheckMeta {
    id: "MEM-12",
    needs: &[],
};

inventory::submit!(Registered(&Mem12));

const FIX: &str = "add `println!(\"cargo:rustc-link-arg=-Tdefmt.x\");` to build.rs, or call bedrock_build::common() \
    which does it (book/issues.md)";

impl Check for Mem12 {
    fn meta(&self) -> &'static CheckMeta {
        &META
    }

    /// `-Tdefmt.x` comes from a `cargo:rustc-link-arg` build script directive (the template's own, or
    /// `bedrock_build::common()`), never from `.cargo/config.toml` rustflags
    fn run(&self, fw: &Firmware) -> Vec<Finding> {
        if !fw.dependencies().iter().any(|d| d.package() == "defmt") {
            return Vec::new();
        }
        let Some(b) = &fw.build_script else {
            return vec![
                Finding::new(
                    "MEM-12",
                    "build.rs",
                    "defmt is a dependency but the crate has no build.rs to add -Tdefmt.x: no .defmt section, defmt messages can't be decoded",
                )
                .span(Span::new(fw.manifest.rel(), 0))
                .fix(FIX)
                .confidence(Confidence::Estimate),
            ];
        };
        let has_flag = b
            .with_key("rustc-link-arg")
            .any(|d| d.value.contains("defmt.x"))
            || b.file.text.contains("bedrock_build::common(");
        if has_flag {
            return Vec::new();
        }
        vec![
            Finding::new(
                "MEM-12",
                "build.rs",
                "defmt is a dependency but build.rs prints no cargo:rustc-link-arg=-Tdefmt.x (and doesn't call \
                 bedrock_build::common()): no .defmt section, defmt messages can't be decoded",
            )
            .span(Span::new(b.file.rel.clone(), 0))
            .fix(FIX)
            .confidence(Confidence::Estimate),
        ]
    }
}
