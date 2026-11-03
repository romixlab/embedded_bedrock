//! `CFG-4` A crate in the tree twice (git and path copies of WireWeaver, embassy): patch it to one source

use crate::check::{Check, CheckMeta, Registered};
use crate::finding::{Finding, Span};
use crate::model::{Firmware, TomlFile};

pub struct Cfg4;

static META: CheckMeta = CheckMeta {
    id: "CFG-4",
    needs: &[],
};

inventory::submit!(Registered(&Cfg4));

fn git_url(v: &toml::Value) -> Option<&str> {
    v.get("git")?.as_str()
}

/// Every `[patch."<url>"]` table across the manifest, the workspace root and the cargo configs: the url and the
/// crate names it patches, with the file that holds it (the first one found)
fn patches<'a>(
    files: impl Iterator<Item = &'a TomlFile>,
) -> Vec<(&'a str, Vec<&'a str>, &'a TomlFile)> {
    let mut out = Vec::new();
    for f in files {
        let Some(patch) = f.table.get("patch").and_then(|v| v.as_table()) else {
            continue;
        };
        for (url, table) in patch {
            if url == "crates-io" {
                continue;
            }
            let Some(names) = table.as_table() else {
                continue;
            };
            out.push((url.as_str(), names.keys().map(String::as_str).collect(), f));
        }
    }
    out
}

const FIX: &str = "add this crate to the same [patch] table as its siblings: a crate reachable both from git and \
    from the patched path links two copies of it (STK-2 note: this exact bug broke every build using the sibling \
    checkout until shrink_wrap was patched too)";

impl Check for Cfg4 {
    fn meta(&self) -> &'static CheckMeta {
        &META
    }

    /// A dependency sourced from git whose source already has an active `[patch]` block (so the workspace means to
    /// take it from a local copy), but is not itself one of the patched crates: it still resolves from git, which
    /// can differ from the patched siblings it shares internals with
    fn run(&self, fw: &Firmware) -> Vec<Finding> {
        let files = std::iter::once(&fw.manifest)
            .chain(fw.workspace.iter())
            .chain(fw.cargo_configs.iter());
        let patches = patches(files);
        if patches.is_empty() {
            return Vec::new();
        }
        let mut out = Vec::new();
        for d in fw.dependencies() {
            let Some(url) =
                git_url(&d.value).or_else(|| d.inherited.as_ref().and_then(|(_, v)| git_url(v)))
            else {
                continue;
            };
            let package = d.package();
            let patched_elsewhere = patches
                .iter()
                .filter(|(u, _, _)| *u == url)
                .any(|(_, names, _)| names.contains(&package));
            if patched_elsewhere {
                continue;
            }
            if let Some((_, _, file)) = patches.iter().find(|(u, _, _)| *u == url) {
                let mut path: Vec<&str> = d.table.iter().map(String::as_str).collect();
                path.push(&d.name);
                out.push(
                    Finding::new(
                        "CFG-4",
                        format!("{}:{}", d.file.rel(), d.name),
                        format!(
                            "{} is taken from git ({url}) while a [patch] on that source patches other crates of \
                             it to a local path: {} is not one of them and still resolves from git",
                            d.name, package
                        ),
                    )
                    .span(Span::new(d.file.rel(), d.file.line_of(&path)))
                    .span(Span::new(file.rel(), 0).label("[patch] on this source"))
                    .fix(FIX),
                );
            }
        }
        out
    }
}
