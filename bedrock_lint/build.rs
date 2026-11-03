//! Embeds the check catalogue (`book/checks.md`) and its detail pages (`book/checks/*.md`), so `explain`,
//! `checks list` and the not-run hints work offline and never drift from the book: writes `$OUT_DIR/book.rs` with
//! an `include_str!` per file.

use std::fmt::Write;
use std::path::Path;
use std::{env, fs};

fn main() {
    let book = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../book")
        .canonicalize()
        .expect("book/ next to the crate");
    let catalogue = book.join("checks.md");
    let pages_dir = book.join("checks");
    println!("cargo::rerun-if-changed={}", catalogue.display());
    // The pages directory may not exist yet (LINT-7, LINT-8 write it)
    let mut pages: Vec<_> = match fs::read_dir(&pages_dir) {
        Ok(dir) => {
            println!("cargo::rerun-if-changed={}", pages_dir.display());
            dir.filter_map(|e| e.ok().map(|e| e.path()))
                .filter(|p| p.extension().is_some_and(|e| e == "md"))
                .collect()
        }
        Err(_) => {
            println!("cargo::rerun-if-changed={}", book.display());
            Vec::new()
        }
    };
    pages.sort();
    let mut out = String::new();
    writeln!(out, "/// `book/checks.md`").unwrap();
    writeln!(
        out,
        "pub static CATALOGUE: &str = include_str!({catalogue:?});"
    )
    .unwrap();
    writeln!(out, "/// `book/checks/<page>.md` by page name").unwrap();
    writeln!(out, "pub static PAGES: &[(&str, &str)] = &[").unwrap();
    for p in &pages {
        println!("cargo::rerun-if-changed={}", p.display());
        let stem = p.file_stem().unwrap().to_str().unwrap();
        writeln!(out, "    ({stem:?}, include_str!({p:?})),").unwrap();
    }
    writeln!(out, "];").unwrap();
    fs::write(
        Path::new(&env::var("OUT_DIR").unwrap()).join("book.rs"),
        out,
    )
    .unwrap();
}
