//! `bedrock-board check` on the model's fixtures and its synthetic chip (stm32-data's format).

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use serde_json::Value;

fn repo() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .to_path_buf()
}

fn run(desc: &Path, chips: &Path, extra: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_bedrock-board"))
        .args(["--color", "never", "check"])
        .arg(desc)
        .arg("--chip-data")
        .arg(chips)
        .args(extra)
        .env_remove("BEDROCK_CHIP_DATA")
        .output()
        .unwrap()
}

fn report(desc: &Path, chips: &Path) -> (i32, Value) {
    let out = run(desc, chips, &["--format", "json"]);
    let v: Value = serde_json::from_slice(&out.stdout)
        .unwrap_or_else(|e| panic!("{e}: {}", String::from_utf8_lossy(&out.stdout)));
    (out.status.code().unwrap(), v)
}

fn codes(v: &Value) -> Vec<(String, String)> {
    v["diagnostics"]
        .as_array()
        .unwrap()
        .iter()
        .map(|d| {
            (
                d["code"].as_str().unwrap().to_owned(),
                d["message"].as_str().unwrap().to_owned(),
            )
        })
        .collect()
}

fn model_fixture(name: &str) -> PathBuf {
    repo().join("bedrock_board_model/tests/fixtures").join(name)
}

#[test]
fn missing_chip_data_skips_the_chip_checks_with_a_note() {
    let empty = tempfile::tempdir().unwrap();
    let (code, v) = report(&model_fixture("minimal.toml"), empty.path());
    assert_eq!(code, 0, "{v:#}");
    let c = codes(&v);
    assert_eq!(c.len(), 1, "{c:?}");
    assert_eq!(c[0].0, "chip-data");
}

#[test]
fn chip_findings_as_text_and_json() {
    let chips = model_fixture("chips");
    let (code, v) = report(&model_fixture("chip.toml"), &chips);
    assert_eq!(code, 1);
    assert_eq!(v["errors"], 5);
    let sig = v["diagnostics"]
        .as_array()
        .unwrap()
        .iter()
        .find(|d| d["code"] == "signal-pin")
        .unwrap();
    assert_eq!(
        (sig["line"].as_u64(), sig["column"].as_u64()),
        (Some(13), Some(27))
    );
    assert_eq!(sig["help"], "pins that carry it: PB1 (AF9)");
    let out = run(&model_fixture("chip.toml"), &chips, &[]);
    let text = String::from_utf8(out.stdout).unwrap();
    assert!(
        text.contains(
            "error[signal-pin]: FDCAN2 TX is not on PA1 (periph.can2 TX)
"
        ),
        "{text}"
    );
    assert!(
        text.contains(
            "  = help: pins that carry it: PB1 (AF9)
"
        ),
        "{text}"
    );
    assert!(
        text.ends_with(
            "T6 STM32TEST: 5 error(s), 0 warning(s)
"
        ),
        "{text}"
    );
}

#[test]
fn invalid_description_fails_and_unreadable_exits_2() {
    let dir = tempfile::tempdir().unwrap();
    let bad = dir.path().join("board.toml");
    std::fs::write(&bad, "[board]\nname = \"X\"\nchip = \"STM32H725IG\"\n\n[gpio.a]\npin = \"PA0\"\nmode = \"output\"\n").unwrap();
    let out = run(&bad, dir.path(), &[]);
    assert_eq!(out.status.code(), Some(1));
    let text = String::from_utf8(out.stdout).unwrap();
    assert!(
        text.contains("error[gpio-fields]: gpio.a: `init` is missing"),
        "{text}"
    );
    assert!(text.contains("board.toml:7:8"), "{text}");
    let out = run(&dir.path().join("none.toml"), dir.path(), &[]);
    assert_eq!(out.status.code(), Some(2));
}
