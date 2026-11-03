//! `MEM-2` Extra RAM section with a tail `. = ALIGN(n)` or a symbol in it

use crate::check::{Check, CheckMeta, Need, Registered};
use crate::finding::{Finding, Span};
use crate::model::Firmware;

pub struct Mem2;

static META: CheckMeta = CheckMeta {
    id: "MEM-2",
    needs: &[Need::MemoryX],
};

inventory::submit!(Registered(&Mem2));

const FIX: &str = "a NOLOAD section must hold only `*(.name .name.*);`: lld keeps an empty section that flip-link \
    counts as used RAM when the location counter advances or a symbol sits in it (flip-link#142)";

impl Check for Mem2 {
    fn meta(&self) -> &'static CheckMeta {
        &META
    }

    /// Walks every `.name (NOLOAD) : ALIGN(n) { ... }` section body and flags the first line in it that isn't
    /// `*(.name .name.*);`
    fn run(&self, fw: &Firmware) -> Vec<Finding> {
        let Some(mx) = &fw.memory_x else {
            return Vec::new();
        };
        let mut out = Vec::new();
        let mut section: Option<(String, bool)> = None; // name, past the opening brace
        for (i, line) in mx.text.lines().enumerate() {
            let t = line.trim();
            let Some((name, in_body)) = &mut section else {
                if t.contains("(NOLOAD)") {
                    let name = t.split_whitespace().next().unwrap_or("").to_string();
                    section = Some((name, false));
                }
                continue;
            };
            if !*in_body {
                if t == "{" {
                    *in_body = true;
                }
                continue;
            }
            if t.starts_with('}') {
                section = None;
                continue;
            }
            if t.is_empty() || t.starts_with("*(") {
                continue;
            }
            out.push(
                Finding::new(
                    "MEM-2",
                    format!("memory.x:{name}"),
                    format!("{name} (NOLOAD) has a line besides `*(...)`: `{t}`"),
                )
                .span(Span::new(mx.rel.clone(), i as u32 + 1))
                .fix(FIX),
            );
            section = None;
        }
        out
    }
}
