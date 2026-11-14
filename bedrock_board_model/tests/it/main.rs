//! Tests of the board description: loading, revisions and reworks, and each check, on the fixtures in
//! `tests/fixtures/` and a synthetic chip (`tests/fixtures/chips/STM32TEST.json`, stm32-data's format).

mod checks;
mod load;

use std::path::{Path, PathBuf};

use bedrock_board_model::{ChipData, Description, Diagnostic, LoadError, Report};

pub(crate) fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name)
}

pub(crate) fn load(name: &str) -> Description {
    Description::load(&fixture(name)).unwrap_or_else(|e| match e {
        LoadError::Invalid { diagnostics, .. } => panic!("{name}: {diagnostics:#?}"),
        other => panic!("{name}: {other}"),
    })
}

pub(crate) fn test_chip() -> ChipData {
    let json = std::fs::read_to_string(fixture("chips/STM32TEST.json")).unwrap();
    ChipData::from_stm32_data(&json).unwrap()
}

/// `(code, line)` of every diagnostic; line 0 for one without a place.
pub(crate) fn codes(desc: &Description, diags: &[Diagnostic]) -> Vec<(&'static str, usize)> {
    diags
        .iter()
        .map(|d| {
            (
                d.code,
                d.span
                    .as_ref()
                    .map_or(0, |s| desc.source.line_col(s.start).0),
            )
        })
        .collect()
}

pub(crate) fn find<'a>(report: &'a Report, code: &str) -> Vec<&'a Diagnostic> {
    report
        .diagnostics
        .iter()
        .filter(|d| d.code == code)
        .collect()
}
