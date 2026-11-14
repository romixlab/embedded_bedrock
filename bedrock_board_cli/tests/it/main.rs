//! `boardmap` driven as a person would.

use std::process::Command;

#[test]
fn boardmap_version_names_the_commit_time() {
    let out = Command::new(env!("CARGO_BIN_EXE_boardmap"))
        .arg("--version")
        .output()
        .unwrap();
    let text = String::from_utf8(out.stdout).unwrap();
    assert!(out.status.success());
    assert!(text.contains(", commit "), "{text}");
    assert!(!text.contains("built"), "{text}");
}
