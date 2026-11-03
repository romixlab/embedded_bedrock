//! `CNT-3` Proc-macro crate and its runtime crate not pinned to the exact same version

use crate::check::{Check, CheckMeta, Registered};
use crate::finding::{Finding, Span};
use crate::model::{Dependency, Firmware};

pub struct Cnt3;

static META: CheckMeta = CheckMeta {
    id: "CNT-3",
    needs: &[],
};

inventory::submit!(Registered(&Cnt3));

const SUFFIXES: &[&str] = &[
    "-macros",
    "_macros",
    "-derive",
    "_derive",
    "-proc-macro",
    "_proc_macro",
];

fn version_req(v: &toml::Value) -> Option<&str> {
    match v {
        toml::Value::String(s) => Some(s.as_str()),
        toml::Value::Table(t) => t.get("version").and_then(|v| v.as_str()),
        _ => None,
    }
}

fn req(d: &Dependency) -> Option<String> {
    version_req(&d.value)
        .or_else(|| d.inherited.as_ref().and_then(|(_, v)| version_req(v)))
        .map(str::to_string)
}

fn span_of(d: &Dependency) -> Span {
    let mut path: Vec<&str> = d.table.iter().map(String::as_str).collect();
    path.push(&d.name);
    Span::new(d.file.rel(), d.file.line_of(&path))
}

impl Check for Cnt3 {
    fn meta(&self) -> &'static CheckMeta {
        &META
    }

    fn run(&self, fw: &Firmware) -> Vec<Finding> {
        let deps = fw.dependencies();
        let mut out = Vec::new();
        for d in &deps {
            let package = d.package();
            let Some(base) = SUFFIXES.iter().find_map(|suf| package.strip_suffix(suf)) else {
                continue;
            };
            let Some(runtime) = deps.iter().find(|r| r.package() == base) else {
                continue;
            };
            let dv = req(d);
            let bv = req(runtime);
            let both_exact = dv.as_deref().is_some_and(|s| s.starts_with('='))
                && bv.as_deref().is_some_and(|s| s.starts_with('='));
            if both_exact && dv == bv {
                continue;
            }
            out.push(
                Finding::new(
                    "CNT-3",
                    format!("{}:{}", d.file.rel(), package),
                    format!(
                        "{package} ({}) and its runtime crate {base} ({}) are not both pinned to the exact same \
                         version: a `cargo update` can resolve them apart, and the pair must match exactly",
                        dv.as_deref().unwrap_or("unpinned"),
                        bv.as_deref().unwrap_or("unpinned")
                    ),
                )
                .span(span_of(d))
                .span(span_of(runtime).label(format!("{base} here")))
                .fix(format!("pin both {package} and {base} to the same `=x.y.z`")),
            );
        }
        out
    }
}
