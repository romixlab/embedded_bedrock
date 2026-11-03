//! `build.rs` and the `cargo:` directives it prints.

use super::{Note, TextFile};
use serde::Serialize;
use syn::punctuated::Punctuated;
use syn::visit::Visit;
use syn::{Expr, Lit, Token};

/// The build script with the directives found in it
#[derive(Clone, Debug, Serialize)]
pub struct BuildScript {
    #[serde(flatten)]
    pub file: TextFile,
    pub directives: Vec<Directive>,
}

/// One `println!("cargo:KEY=VALUE")` (or `cargo::KEY=VALUE`) with a literal format string
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Directive {
    /// `rustc-link-arg`
    pub key: String,
    /// What follows `=`, format placeholders as written (`{}`)
    pub value: String,
    /// The two-colon form `cargo::`, where an unknown key is an error instead of metadata
    pub double_colon: bool,
    pub line: u32,
    pub column: u32,
}

/// Keys Cargo understands as build script instructions; with one colon anything else is `links` metadata, which
/// Cargo silently passes on (or drops, without `links`)
pub const KNOWN_KEYS: &[&str] = &[
    "rerun-if-changed",
    "rerun-if-env-changed",
    "rustc-link-arg",
    "rustc-link-arg-bin",
    "rustc-link-arg-bins",
    "rustc-link-arg-tests",
    "rustc-link-arg-examples",
    "rustc-link-arg-benches",
    "rustc-link-arg-cdylib",
    "rustc-cdylib-link-arg",
    "rustc-link-lib",
    "rustc-link-search",
    "rustc-flags",
    "rustc-cfg",
    "rustc-check-cfg",
    "rustc-env",
    "warning",
    "error",
    "metadata",
];

impl BuildScript {
    pub fn parse(file: TextFile, notes: &mut Vec<Note>) -> BuildScript {
        let mut directives = Vec::new();
        match syn::parse_file(&file.text) {
            Ok(ast) => {
                let mut v = Printer {
                    out: &mut directives,
                };
                v.visit_file(&ast);
            }
            Err(e) => notes.push(Note {
                file: file.rel.clone(),
                message: format!(
                    "Rust syntax error at {}:{}: {e}; its directives are missing",
                    e.span().start().line,
                    e.span().start().column + 1
                ),
            }),
        }
        BuildScript { file, directives }
    }

    /// The directives with this key
    pub fn with_key<'a>(&'a self, key: &'a str) -> impl Iterator<Item = &'a Directive> {
        self.directives.iter().filter(move |d| d.key == key)
    }
}

struct Printer<'a> {
    out: &'a mut Vec<Directive>,
}

impl<'ast> Visit<'ast> for Printer<'_> {
    fn visit_macro(&mut self, mac: &'ast syn::Macro) {
        let name = mac.path.segments.last().map(|s| s.ident.to_string());
        if matches!(name.as_deref(), Some("println" | "print")) {
            if let Ok(args) = mac.parse_body_with(Punctuated::<Expr, Token![,]>::parse_terminated)
                && let Some(Expr::Lit(lit)) = args.first()
                && let Lit::Str(s) = &lit.lit
            {
                let text = s.value();
                let start = s.span().start();
                for line in text.lines() {
                    if let Some(d) = directive(line, start.line as u32, start.column as u32 + 1) {
                        self.out.push(d);
                    }
                }
            }
        } else if let Ok(args) =
            mac.parse_body_with(Punctuated::<Expr, Token![,]>::parse_terminated)
        {
            // directives printed inside other macros' arguments (rare, but `if cfg!(..) { println!(..) }` is not
            // a macro argument and is visited anyway)
            for a in &args {
                self.visit_expr(a);
            }
        }
    }
}

/// `cargo:rustc-link-arg=-Tlink.x` → key `rustc-link-arg`, value `-Tlink.x`
fn directive(line: &str, l: u32, c: u32) -> Option<Directive> {
    let (rest, double_colon) = if let Some(r) = line.strip_prefix("cargo::") {
        (r, true)
    } else {
        (line.strip_prefix("cargo:")?, false)
    };
    let (key, value) = rest.split_once('=').unwrap_or((rest, ""));
    Some(Directive {
        key: key.to_string(),
        value: value.to_string(),
        double_colon,
        line: l,
        column: c,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn finds_directives() {
        let text = r#"
fn main() {
    println!("cargo:rustc-link-search={}", out.display());
    println!("cargo::rerun-if-changed=memory.x");
    if true {
        println!("cargo:rustc-linker=flip-link");
    }
    let s = format!("cargo:not-printed=1");
}
"#;
        let mut notes = Vec::new();
        let b = BuildScript::parse(
            TextFile {
                rel: "build.rs".into(),
                path: PathBuf::new(),
                text: text.into(),
            },
            &mut notes,
        );
        assert!(notes.is_empty());
        let keys: Vec<_> = b
            .directives
            .iter()
            .map(|d| (d.key.as_str(), d.line))
            .collect();
        assert_eq!(
            keys,
            [
                ("rustc-link-search", 3),
                ("rerun-if-changed", 4),
                ("rustc-linker", 6)
            ]
        );
        assert!(b.directives[1].double_colon);
        assert_eq!(b.directives[2].value, "flip-link");
    }
}
