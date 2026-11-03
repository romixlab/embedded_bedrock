//! `STK-4` cortex-m-rt `paint-stack` on before its BusFault is understood

use crate::check::{Check, CheckMeta, Registered};
use crate::finding::{Confidence, Finding, Span};
use crate::model::Firmware;

pub struct Stk4;

static META: CheckMeta = CheckMeta {
    id: "STK-4",
    needs: &[],
};

inventory::submit!(Registered(&Stk4));

const FIX: &str = "drop the `paint-stack` feature of cortex-m-rt; turn it on again only once the BusFault it caused on usb_io_fw (a01319a) is understood for this chip, and waive this with that reason";

impl Check for Stk4 {
    fn meta(&self) -> &'static CheckMeta {
        &META
    }

    /// The feature in a cortex-m-rt dependency (the crate's or the workspace entry it inherits), or in one of the
    /// crate's own features as `cortex-m-rt/paint-stack`: certain when that feature is `default`, an estimate
    /// otherwise (it is on only when the feature is chosen)
    fn run(&self, fw: &Firmware) -> Vec<Finding> {
        let mut out = Vec::new();
        let mut dep_names = vec!["cortex-m-rt".to_string()];
        for d in fw.dependencies() {
            if d.package() != "cortex-m-rt" {
                continue;
            }
            dep_names.push(d.name.clone());
            for (feature, file, line) in d.features() {
                if feature == "paint-stack" {
                    out.push(
                        Finding::new(
                            "STK-4",
                            format!("{}:{}:paint-stack", file.rel(), d.name),
                            "cortex-m-rt's paint-stack is on: it caused a BusFault at boot on usb_io_fw",
                        )
                        .span(Span::new(file.rel(), line))
                        .fix(FIX),
                    );
                }
            }
        }
        if let Some(features) = fw.manifest.get(&["features"]).and_then(|f| f.as_table()) {
            for (name, list) in features {
                let Some(list) = list.as_array() else {
                    continue;
                };
                let on = list.iter().filter_map(|v| v.as_str()).any(|v| {
                    v.split_once('/').is_some_and(|(dep, f)| {
                        dep_names.iter().any(|n| n == dep.trim_end_matches('?'))
                            && f == "paint-stack"
                    })
                });
                if on {
                    let mut f = Finding::new(
                        "STK-4",
                        format!("Cargo.toml:features.{name}:paint-stack"),
                        if name == "default" {
                            "cortex-m-rt's paint-stack is on by default: it caused a BusFault at boot on usb_io_fw".to_string()
                        } else {
                            format!("feature `{name}` turns on cortex-m-rt's paint-stack: it caused a BusFault at boot on usb_io_fw")
                        },
                    )
                    .span(Span::new(fw.manifest.rel(), fw.manifest.line_of(&["features", name])))
                    .fix(FIX);
                    if name != "default" {
                        f = f.confidence(Confidence::Estimate);
                    }
                    out.push(f);
                }
            }
        }
        out
    }
}
