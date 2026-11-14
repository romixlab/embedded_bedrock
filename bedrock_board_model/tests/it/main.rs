//! Tests of the board description: loading, revisions and reworks, on the fixtures in `tests/fixtures/`.

mod load;

use std::path::{Path, PathBuf};

use bedrock_board_model::{Description, LoadError};

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
