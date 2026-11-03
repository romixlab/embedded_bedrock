//! `CFG-7` Dirty build flashed without a mark

use crate::check::{Check, CheckMeta, Registered};
use crate::finding::{Confidence, Finding, Span};
use crate::model::Firmware;

pub struct Cfg7;

static META: CheckMeta = CheckMeta {
    id: "CFG-7",
    needs: &[],
};

inventory::submit!(Registered(&Cfg7));

impl Check for Cfg7 {
    fn meta(&self) -> &'static CheckMeta {
        &META
    }

    /// No `bedrock_build` dependency: the firmware embeds no git SHA or dirty flag, so a dirty build flashed in a
    /// hurry can't be told apart from a clean one once it is on the device
    fn run(&self, fw: &Firmware) -> Vec<Finding> {
        if fw
            .dependencies()
            .iter()
            .any(|d| d.package() == "bedrock_build")
        {
            return Vec::new();
        }
        vec![
            Finding::new(
                "CFG-7",
                "bedrock_build",
                "no bedrock_build dependency: the build carries no git SHA or dirty flag, so a dirty build \
                 flashed by mistake can't be told apart from a clean one by reading the device",
            )
            .span(Span::new(fw.manifest.rel(), 0))
            .fix("add bedrock_build as a build-dependency and call serialize_build_info() from build.rs (book/build_info.md)")
            .confidence(Confidence::Estimate),
        ]
    }
}
