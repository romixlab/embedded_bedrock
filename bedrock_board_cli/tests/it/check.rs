//! `bedrock-board check` on B135's description and on fixtures, with STM32H725IG chip data written from
//! stm32-metapac's metadata in stm32-data's format (no download, no cache of the host).

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use serde_json::{Value, json};

fn repo() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .to_path_buf()
}

fn b135() -> PathBuf {
    repo().join("bedrock_board_model/tests/boards/b135/board.toml")
}

/// A directory with `STM32H725IG.json` in stm32-data's chip format, from the metapac (no packages there).
fn chip_dir() -> tempfile::TempDir {
    use stm32_metapac::metadata::METADATA;
    let peripherals: Vec<Value> = METADATA
        .peripherals
        .iter()
        .map(|p| {
            let pins: Vec<Value> = p
                .pins
                .iter()
                .map(|s| json!({ "pin": s.pin, "signal": s.signal, "af": s.af }))
                .collect();
            json!({ "name": p.name, "pins": pins })
        })
        .collect();
    let pins: Vec<Value> = METADATA
        .pins
        .iter()
        .map(|p| json!({ "name": p.name }))
        .collect();
    let chip = json!({ "name": "STM32H725IG", "cores": [{ "name": "cm7", "pins": pins, "peripherals": peripherals }] });
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("STM32H725IG.json"), chip.to_string()).unwrap();
    dir
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

#[test]
fn b135_is_clean_on_both_revisions() {
    let chips = chip_dir();
    let (code, v) = report(&b135(), chips.path());
    assert_eq!(code, 0, "{v:#}");
    assert_eq!(v["configurations"], json!(["rev A", "rev B"]));
    assert_eq!(v["errors"], 0);
    assert_eq!(v["warnings"], 0);
    // the metapac has no package data: one note that pins were checked against the die
    assert_eq!(
        codes(&v).iter().map(|c| c.0.as_str()).collect::<Vec<_>>(),
        ["package-data"]
    );
}

#[test]
fn b135b_pins_of_j203_were_missing_from_the_preset() {
    // the description as first written from the preset and the board map: PA0_C and PA1_C (J203-1 / J203-2) unnamed
    let text = std::fs::read_to_string(b135()).unwrap();
    let text: String = text
        .lines()
        .filter(|l| !l.starts_with("PA0_C") && !l.starts_with("PA1_C"))
        .collect::<Vec<_>>()
        .join("\n")
        .replace(", \"free.PA0_C\", \"free.PA1_C\"", "");
    // in a scratch directory, the PCB paths made absolute
    let dir = b135().parent().unwrap().to_path_buf();
    let text = text
        .replace("pcb = \"../", &format!("pcb = \"{}/../", dir.display()))
        .replace("pcb = \"b135a", &format!("pcb = \"{}/b135a", dir.display()));
    let scratch = tempfile::tempdir().unwrap();
    let copy = scratch.path().join("board.toml");
    std::fs::write(&copy, text).unwrap();
    let chips = chip_dir();
    let (code, v) = report(&copy, chips.path());
    assert_eq!(code, 1);
    let found: Vec<_> = codes(&v)
        .into_iter()
        .filter(|c| c.0 != "package-data")
        .collect();
    assert_eq!(
        found,
        [
            (
                "pcb-undescribed".to_owned(),
                "PA0_C is on net /MCU_H7/PA0_C, but the description does not name it".to_owned()
            ),
            (
                "pcb-undescribed".to_owned(),
                "PA1_C is on net /MCU_H7/PA1_C, but the description does not name it".to_owned()
            ),
        ]
    );
    assert_eq!(v["diagnostics"][1]["configs"], json!(["rev B"]));
}

#[test]
fn b135_firmware_today_drives_rev_a_leds_on_rev_b() {
    let chips = chip_dir();
    let fixture =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/it/fixtures/b135b-fw-today.toml");
    let (code, v) = report(&fixture, chips.path());
    assert_eq!(code, 1);
    let errors: Vec<String> = v["diagnostics"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|d| d["severity"] == "error" && d["code"] != "pcb-shared")
        .map(|d| d["message"].as_str().unwrap().to_owned())
        .collect();
    assert_eq!(
        errors,
        [
            "PD14 (led.sd red) is not connected on the PCB",
            "PD13 (led.sd green) is not connected on the PCB",
            "PD12 (led.sd blue) is on net /MCU_H7/PA7_PD12, not LED_SD_B",
            "PA10 (led.can blue) is not connected on the PCB",
        ]
    );
}

#[test]
fn text_report_points_at_the_line() {
    let chips = chip_dir();
    let fixture =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/it/fixtures/b135b-fw-today.toml");
    let out = run(&fixture, chips.path(), &[]);
    let text = String::from_utf8(out.stdout).unwrap();
    assert!(
        text.contains("error[pcb-unconnected]: PA10 (led.can blue) is not connected on the PCB\n"),
        "{text}"
    );
    assert!(text.contains("b135b-fw-today.toml:19:22"), "{text}");
    assert!(
        text.contains("rgb = [\"PC7\", \"PC6\", \"PA10\"]\n"),
        "{text}"
    );
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
