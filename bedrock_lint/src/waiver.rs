//! Waivers in the firmware's own files: `// bedrock-lint: allow(EXE-1) <reason>` in Rust, `# bedrock-lint:
//! allow(STK-4) <reason>` in TOML.
//!
//! - At the end of a line, a waiver covers that line.
//! - On a line of its own, it covers the next item: the first line after it that is not blank, a comment or an
//!   attribute, and in Rust the whole item or statement that starts there (a waiver above `fn main` covers findings
//!   inside `main`); in TOML a table header covers its table.
//! - The reason is required: a waiver without one, or naming no check of the catalogue, is a [`Problem`] that fails
//!   the run.

use crate::finding::Finding;
use crate::model::Firmware;
use serde::Serialize;
use syn::spanned::Spanned;
use syn::visit::{self, Visit};

const MARK: &str = "bedrock-lint:";

/// One waiver comment
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Waiver {
    pub file: String,
    /// The line of the comment
    pub line: u32,
    pub checks: Vec<String>,
    pub reason: String,
    /// The lines it covers, inclusive
    pub covers: (u32, u32),
}

/// A waiver that is written wrong
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Problem {
    pub file: String,
    pub line: u32,
    pub message: String,
}

/// The waivers of every file of the model, and the ones written wrong
pub fn collect(fw: &Firmware) -> (Vec<Waiver>, Vec<Problem>) {
    let mut waivers = Vec::new();
    let mut problems = Vec::new();
    let mut files: Vec<(&str, &str, Option<&syn::File>)> = fw
        .sources
        .iter()
        .map(|s| (s.rel.as_str(), s.text.as_str(), Some(&s.ast)))
        .collect();
    for f in std::iter::once(&fw.manifest.file)
        .chain(fw.workspace.iter().map(|w| &w.file))
        .chain(fw.cargo_configs.iter().map(|c| &c.file))
        .chain(fw.build_script.iter().map(|b| &b.file))
        .chain(fw.memory_x.iter())
    {
        files.push((f.rel.as_str(), f.text.as_str(), None));
    }
    for (rel, text, ast) in files {
        parse_file(rel, text, ast, &mut waivers, &mut problems);
    }
    (waivers, problems)
}

fn parse_file(
    rel: &str,
    text: &str,
    ast: Option<&syn::File>,
    waivers: &mut Vec<Waiver>,
    problems: &mut Vec<Problem>,
) {
    if !text.contains(MARK) {
        return;
    }
    let rust = rel.ends_with(".rs");
    let lines: Vec<&str> = text.lines().collect();
    let extents = ast.map(extents).unwrap_or_default();
    for (i, line) in lines.iter().enumerate() {
        let Some(at) = line.find(MARK) else { continue };
        let before = &line[..at];
        let comment = if rust {
            before.trim_end().ends_with("//") || before.contains("//")
        } else {
            before.contains('#')
        };
        if !comment {
            continue;
        }
        let code_before = if rust {
            before.split("//").next().unwrap_or("").trim()
        } else {
            before.split('#').next().unwrap_or("").trim()
        };
        let lineno = i as u32 + 1;
        let rest = line[at + MARK.len()..].trim();
        let problem = |message: String| Problem {
            file: rel.to_string(),
            line: lineno,
            message,
        };
        let Some(args) = rest.strip_prefix("allow(") else {
            problems.push(problem(format!(
                "waiver must read `{MARK} allow(ID) reason`, found `{rest}`"
            )));
            continue;
        };
        let Some((ids, reason)) = args.split_once(')') else {
            problems.push(problem("waiver has no closing `)`".into()));
            continue;
        };
        let checks: Vec<String> = ids
            .split(',')
            .map(|s| s.trim().to_ascii_uppercase())
            .filter(|s| !s.is_empty())
            .collect();
        let reason = reason.trim().trim_start_matches(['-', ':', '—']).trim();
        let cat = crate::catalogue::catalogue();
        if checks.is_empty() {
            problems.push(problem("waiver names no check".into()));
            continue;
        }
        if let Some(bad) = checks.iter().find(|c| cat.entry(c).is_none()) {
            problems.push(problem(format!(
                "waiver names {bad}, which is no check of the catalogue"
            )));
            continue;
        }
        if reason.is_empty() {
            problems.push(problem(format!(
                "waiver of {} gives no reason: say why the finding is acceptable",
                checks.join(", ")
            )));
            continue;
        }
        let covers = if !code_before.is_empty() {
            (lineno, lineno)
        } else {
            let target = lines
                .iter()
                .enumerate()
                .skip(i + 1)
                .find(|(_, l)| {
                    let t = l.trim();
                    !(t.is_empty()
                        || if rust {
                            t.starts_with("//") || t.starts_with("#[") || t.starts_with("#![")
                        } else {
                            t.starts_with('#')
                        })
                })
                .map(|(j, _)| j as u32 + 1);
            match target {
                None => (lineno, lineno),
                Some(t) if rust => {
                    // nodes starting between the comment and the target: the item with its attributes
                    let end = extents
                        .iter()
                        .filter(|(s, _)| *s > lineno && *s <= t)
                        .map(|(_, e)| *e)
                        .max()
                        .unwrap_or(t);
                    (t, end)
                }
                Some(t) => {
                    let header = lines[t as usize - 1].trim_start().starts_with('[');
                    let end = if header {
                        lines
                            .iter()
                            .enumerate()
                            .skip(t as usize)
                            .find(|(_, l)| l.trim_start().starts_with('['))
                            .map_or(lines.len() as u32, |(j, _)| j as u32)
                    } else {
                        t
                    };
                    (t, end)
                }
            }
        };
        waivers.push(Waiver {
            file: rel.to_string(),
            line: lineno,
            checks,
            reason: reason.to_string(),
            covers,
        });
    }
}

/// The waiver that covers a finding: same file as its main span, the check named, the line covered
pub fn find<'w>(waivers: &'w [Waiver], f: &Finding) -> Option<&'w Waiver> {
    let span = f.primary()?;
    waivers.iter().find(|w| {
        w.file == span.file
            && w.checks.contains(&f.check)
            && (w.covers.0..=w.covers.1).contains(&span.line)
    })
}

/// (start line, end line) of every item and statement
fn extents(ast: &syn::File) -> Vec<(u32, u32)> {
    struct V(Vec<(u32, u32)>);
    impl V {
        fn push(&mut self, span: proc_macro2::Span) {
            self.0
                .push((span.start().line as u32, span.end().line as u32));
        }
    }
    impl<'ast> Visit<'ast> for V {
        fn visit_item(&mut self, i: &'ast syn::Item) {
            self.push(i.span());
            visit::visit_item(self, i);
        }
        fn visit_impl_item(&mut self, i: &'ast syn::ImplItem) {
            self.push(i.span());
            visit::visit_impl_item(self, i);
        }
        fn visit_stmt(&mut self, s: &'ast syn::Stmt) {
            self.push(s.span());
            visit::visit_stmt(self, s);
        }
    }
    let mut v = V(Vec::new());
    v.visit_file(ast);
    v.0
}

#[cfg(test)]
mod tests {
    use super::*;

    fn waivers(rel: &str, text: &str) -> (Vec<Waiver>, Vec<Problem>) {
        let ast = rel.ends_with(".rs").then(|| syn::parse_file(text).unwrap());
        let mut w = Vec::new();
        let mut p = Vec::new();
        parse_file(rel, text, ast.as_ref(), &mut w, &mut p);
        (w, p)
    }

    #[test]
    fn rust_waivers_cover_the_next_item() {
        let (w, p) = waivers(
            "src/main.rs",
            "// bedrock-lint: allow(exe-3) priority set by the ROM\n#[entry]\nfn main() {\n    a();\n}\nfn b() {} // bedrock-lint: allow(STK-2, EXE-1) - trailing\n",
        );
        assert!(p.is_empty(), "{p:?}");
        assert_eq!(w[0].checks, ["EXE-3"]);
        assert_eq!(w[0].reason, "priority set by the ROM");
        assert_eq!(w[0].covers, (3, 5));
        assert_eq!(w[1].covers, (6, 6));
        assert_eq!(w[1].reason, "trailing");
    }

    #[test]
    fn toml_waivers_cover_a_table() {
        let (w, _) = waivers(
            "Cargo.toml",
            "# bedrock-lint: allow(STK-4) checked\n[dependencies]\na = 1\nb = 2\n[x]\n",
        );
        assert_eq!(w[0].covers, (2, 4));
    }

    #[test]
    fn problems() {
        let (w, p) = waivers(
            "src/main.rs",
            "// bedrock-lint: allow(EXE-3)\n// bedrock-lint: allow(NOPE-1) x\n// bedrock-lint: deny(EXE-3)\nfn main() {}\n",
        );
        assert!(w.is_empty());
        assert_eq!(p.len(), 3);
        assert!(p[0].message.contains("no reason"));
        assert!(p[1].message.contains("NOPE-1"));
    }
}
