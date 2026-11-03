//! `bedrock-lint` driven through its binary, on copies of the library's fixtures

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../bedrock_lint/tests/fixtures")
}

/// The binary, without the CARGO_* variables of the test run and without colour
fn bin(args: &[&str]) -> Output {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_bedrock-lint"));
    for (k, _) in std::env::vars() {
        if k.starts_with("CARGO") || k == "COMPLETE" {
            cmd.env_remove(k);
        }
    }
    cmd.env("NO_COLOR", "1").args(args).output().unwrap()
}

fn stdout(o: &Output) -> String {
    String::from_utf8_lossy(&o.stdout).into_owned()
}

/// A copy of a fixture in a scratch directory, which is then the discovery root
fn copy(name: &str) -> tempfile::TempDir {
    fn cp(from: &Path, to: &Path) {
        std::fs::create_dir_all(to).unwrap();
        for e in std::fs::read_dir(from).unwrap() {
            let e = e.unwrap();
            let t = to.join(e.file_name());
            if e.file_type().unwrap().is_dir() {
                cp(&e.path(), &t);
            } else {
                std::fs::copy(e.path(), t).unwrap();
            }
        }
    }
    let d = tempfile::tempdir().unwrap();
    cp(&fixtures().join(name), d.path());
    d
}

#[test]
fn version_names_the_commit() {
    let o = bin(&["--version"]);
    let s = stdout(&o);
    assert!(s.starts_with("bedrock-lint 0."), "{s}");
    assert!(s.contains(", commit "), "{s}");
}

#[test]
fn exit_status_follows_findings() {
    let d = copy("exe3_order");
    let dir = d.path().to_str().unwrap();
    let o = bin(&["check", dir, "--root", dir, "--only", "EXE-3"]);
    assert_eq!(o.status.code(), Some(1), "{}", stdout(&o));
    let s = stdout(&o);
    assert!(s.contains("error[EXE-3]"), "{s}");
    assert!(!s.contains('\x1b'), "no colour with NO_COLOR: {s}");

    let clean = copy("clean");
    let dir = clean.path().to_str().unwrap();
    let o = bin(&["check", dir, "--root", dir]);
    assert_eq!(o.status.code(), Some(0), "{}", stdout(&o));
    assert!(stdout(&o).contains("4 checks ran"), "{}", stdout(&o));

    let o = bin(&["check", "/nonexistent/fw"]);
    assert_eq!(o.status.code(), Some(2));
    let o = bin(&["check", dir, "--only", "NOPE-9"]);
    assert_eq!(o.status.code(), Some(2));
}

#[test]
fn baseline_flow() {
    let d = copy("stk2_rustc_linker");
    let dir = d.path().to_str().unwrap();
    let base = d.path().join("bedrock-lint.baseline.json");
    let o = bin(&[
        "check",
        dir,
        "--root",
        dir,
        "--write-baseline",
        base.to_str().unwrap(),
    ]);
    assert_eq!(o.status.code(), Some(0), "{}", stdout(&o));
    assert!(stdout(&o).contains("wrote 3 finding(s)"), "{}", stdout(&o));
    // named in bedrock-lint.toml: the known findings no longer fail
    std::fs::write(
        d.path().join("bedrock-lint.toml"),
        "baseline = \"bedrock-lint.baseline.json\"\n",
    )
    .unwrap();
    let o = bin(&["check", dir, "--root", dir]);
    assert_eq!(o.status.code(), Some(0), "{}", stdout(&o));
    assert!(
        stdout(&o).contains("In the baseline (3):"),
        "{}",
        stdout(&o)
    );
    // a new finding fails
    let b = std::fs::read_to_string(d.path().join("build.rs")).unwrap();
    std::fs::write(
        d.path().join("build.rs"),
        b.replace(
            "fn main() {",
            "fn main() {\n    println!(\"cargo:rustc-linker=lld\");",
        ),
    )
    .unwrap();
    let o = bin(&["check", dir, "--root", dir]);
    assert_eq!(o.status.code(), Some(1), "{}", stdout(&o));
    assert!(stdout(&o).contains("error[STK-2] new"), "{}", stdout(&o));
}

#[test]
fn machine_formats() {
    let d = copy("exe3_order");
    let dir = d.path().to_str().unwrap();
    let o = bin(&["check", dir, "--root", dir, "--format", "json"]);
    let v: serde_json::Value = serde_json::from_slice(&o.stdout).unwrap();
    assert_eq!(v["firmware"], "exe3_order");
    let sarif = d.path().join("out.sarif");
    let o = bin(&[
        "check",
        dir,
        "--root",
        dir,
        "--format",
        "sarif",
        "-o",
        sarif.to_str().unwrap(),
    ]);
    assert!(
        stdout(&o).contains("error[EXE-3]"),
        "the text report still shows"
    );
    let v: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(sarif).unwrap()).unwrap();
    assert_eq!(v["version"], "2.1.0");
    assert!(!v["runs"][0]["results"].as_array().unwrap().is_empty());
}

#[test]
fn explain_and_list() {
    let o = bin(&["explain", "exe-3"]);
    let s = stdout(&o);
    assert!(
        s.contains("EXE-3 Interrupt executor started before its IRQ priority is set"),
        "{s}"
    );
    assert!(s.contains("bedrock-lint: automated"), "{s}");
    let s = stdout(&bin(&["explain", "STK"]));
    assert!(s.contains("STK-7"), "{s}");
    let s = stdout(&bin(&["checks", "list"]));
    assert!(
        s.contains("EXE-3") && s.contains("STK-1") && !s.contains("EXE-1 "),
        "{s}"
    );
    let s = stdout(&bin(&["checks", "list", "--all", "--category", "EXE"]));
    assert!(s.contains("EXE-1") && s.contains("not yet"), "{s}");
    let v: serde_json::Value =
        serde_json::from_slice(&bin(&["checks", "list", "--json"]).stdout).unwrap();
    assert_eq!(v.as_array().unwrap().len(), 4);
}

#[test]
fn completes_check_ids_with_titles() {
    let o = Command::new(env!("CARGO_BIN_EXE_bedrock-lint"))
        .env("COMPLETE", "fish")
        .args(["--", "bedrock-lint", "explain", "EXE-"])
        .output()
        .unwrap();
    let s = stdout(&o);
    assert!(s.contains("EXE-3\tInterrupt executor started"), "{s}");
}
