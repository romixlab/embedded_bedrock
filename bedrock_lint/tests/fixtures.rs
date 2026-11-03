//! Each directory under `tests/fixtures/` is a small firmware with an `expect.toml`: the checks to run (`checks`,
//! all when absent), the findings they must give (`[[finding]]`: check, place, confidence, waived) and nothing else,
//! the checks that must not run (`not_run`), must run (`ran`), waivers written wrong (`problems`), tasks the model
//! must see (`tasks`). `dir` is the crate inside the fixture when it is not the fixture itself; the fixture is the
//! discovery root. An `elf.toml` describes the linked ELF ([`synth`]): the test writes it and passes it as `--elf`
//! would. A `bedrock-lint.toml` in the crate is the project's configuration (the thresholds of the elf checks).

mod synth;

use bedrock_lint::{
    Baseline, Confidence, FailOn, Firmware, LoadOptions, RunOptions, Severity, run,
};
use serde::Deserialize;
use std::path::{Path, PathBuf};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Expect {
    #[serde(default)]
    checks: Vec<String>,
    dir: Option<String>,
    #[serde(default)]
    finding: Vec<ExpectFinding>,
    #[serde(default)]
    not_run: Vec<String>,
    #[serde(default)]
    ran: Vec<String>,
    #[serde(default)]
    problems: Vec<String>,
    #[serde(default)]
    tasks: Vec<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ExpectFinding {
    check: String,
    /// `file`, `file:line` or `file:line:column` of the main span
    at: String,
    #[serde(default)]
    confidence: Option<String>,
    #[serde(default)]
    waived: bool,
}

fn fixtures() -> Vec<PathBuf> {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
    let mut v: Vec<PathBuf> = std::fs::read_dir(dir)
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| p.join("expect.toml").is_file())
        .collect();
    v.sort();
    v
}

fn load(fixture: &Path) -> (Firmware, Expect) {
    let expect: Expect =
        toml::from_str(&std::fs::read_to_string(fixture.join("expect.toml")).unwrap()).unwrap();
    let dir = expect
        .dir
        .as_ref()
        .map_or(fixture.to_path_buf(), |d| fixture.join(d));
    let elf = fixture.join("elf.toml").is_file().then(|| {
        let spec: synth::Spec =
            toml::from_str(&std::fs::read_to_string(fixture.join("elf.toml")).unwrap()).unwrap();
        let name = fixture.file_name().unwrap().to_string_lossy().into_owned();
        let out = Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!("{name}.elf"));
        std::fs::write(&out, synth::write(&spec)).unwrap();
        out
    });
    let fw = Firmware::load(
        &dir,
        &LoadOptions {
            root: Some(fixture.to_path_buf()),
            elf,
        },
    )
    .unwrap();
    (fw, expect)
}

fn place(s: &bedrock_lint::Span, at: &str) -> String {
    match at.matches(':').count() {
        0 => s.file.clone(),
        1 => format!("{}:{}", s.file, s.line),
        _ => format!("{}:{}:{}", s.file, s.line, s.column),
    }
}

#[test]
fn fixtures_give_their_findings() {
    let mut failures = Vec::new();
    for fixture in fixtures() {
        let name = fixture.file_name().unwrap().to_string_lossy().into_owned();
        let (fw, expect) = load(&fixture);
        let report = run(
            &fw,
            &RunOptions {
                only: expect.checks.clone(),
                config: bedrock_lint::Config::load(&fw.dir).unwrap(),
                ..Default::default()
            },
        )
        .unwrap();
        let elf_file = fw.elf.as_ref().map(|e| e.rel.clone());
        let mut got: Vec<String> = report
            .findings
            .iter()
            .map(|f| {
                // a span in the written ELF is `<elf>` in expect.toml
                let mut f = f.clone();
                for s in &mut f.spans {
                    if Some(&s.file) == elf_file.as_ref() {
                        s.file = "<elf>".into();
                    }
                }
                // the form of an expected place of this check that matches, else of the first
                let ats: Vec<&str> = expect
                    .finding
                    .iter()
                    .filter(|e| e.check == f.check)
                    .map(|e| e.at.as_str())
                    .collect();
                let at = f
                    .primary()
                    .and_then(|s| ats.iter().find(|at| place(s, at) == **at))
                    .or(ats.first())
                    .copied()
                    .unwrap_or(":");
                format!(
                    "{} {} {}{}",
                    f.check,
                    f.primary().map(|s| place(s, at)).unwrap_or_default(),
                    f.confidence.name(),
                    if f.waived.is_some() { " waived" } else { "" }
                )
            })
            .collect();
        let mut want: Vec<String> = expect
            .finding
            .iter()
            .map(|e| {
                format!(
                    "{} {} {}{}",
                    e.check,
                    e.at,
                    e.confidence.as_deref().unwrap_or("certain"),
                    if e.waived { " waived" } else { "" }
                )
            })
            .collect();
        got.sort();
        want.sort();
        if got != want {
            failures.push(format!(
                "{name}: findings\n  got  {got:?}\n  want {want:?}\n{}",
                bedrock_lint::report::text::render(&report, Some(&fw), FailOn::Never, false)
            ));
        }
        let not_run: Vec<&str> = report.not_run.iter().map(|n| n.check.as_str()).collect();
        if not_run != expect.not_run {
            failures.push(format!(
                "{name}: not run {not_run:?}, want {:?}",
                expect.not_run
            ));
        }
        for r in &expect.ran {
            if !report.ran.contains(r) {
                failures.push(format!("{name}: {r} did not run ({:?})", report.not_run));
            }
        }
        let problems: Vec<String> = report
            .problems
            .iter()
            .map(|p| format!("{}:{}", p.file, p.line))
            .collect();
        if problems != expect.problems {
            failures.push(format!(
                "{name}: problems {problems:?}, want {:?}",
                expect.problems
            ));
        }
        for t in &expect.tasks {
            if !fw.facts.tasks.iter().any(|x| x.path == *t) {
                failures.push(format!("{name}: no task {t} in {:?}", fw.facts.tasks));
            }
        }
        if !fw.notes.is_empty() {
            failures.push(format!("{name}: loader notes {:?}", fw.notes));
        }
    }
    assert!(failures.is_empty(), "\n{}", failures.join("\n\n"));
}

#[test]
fn every_registered_check_has_a_fixture_and_is_in_the_catalogue() {
    let cat = bedrock_lint::catalogue::catalogue();
    let fixtures: Vec<String> = fixtures()
        .iter()
        .map(|p| std::fs::read_to_string(p.join("expect.toml")).unwrap())
        .collect();
    for c in bedrock_lint::registered() {
        let id = c.meta().id;
        let e = cat
            .entry(id)
            .unwrap_or_else(|| panic!("{id} is not in book/checks.md"));
        assert!(
            e.automatic(),
            "{id} is registered but the catalogue says `check: {}`",
            e.check
        );
        assert!(
            fixtures
                .iter()
                .any(|f| f.contains(&format!("check = \"{id}\""))),
            "{id} has no fixture with a finding"
        );
    }
}

#[test]
fn baseline_holds_back_known_findings() {
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/exe3_order");
    let (fw, _) = load(&fixture);
    let opts = RunOptions {
        only: vec!["EXE-3".into()],
        ..Default::default()
    };
    let first = run(&fw, &opts).unwrap();
    assert!(first.fails(FailOn::Severity(Severity::Warning)));
    let baseline = Baseline::from_findings(&first.findings);
    assert_eq!(baseline.findings.len(), 3, "the waived finding stays out");
    let round: Baseline = serde_json::from_str(&baseline.to_json()).unwrap();
    assert_eq!(round, baseline);

    let mut known = baseline.clone();
    known.findings.pop();
    let second = run(
        &fw,
        &RunOptions {
            baseline: Some(known),
            ..opts.clone()
        },
    )
    .unwrap();
    let failing = second.failing(FailOn::Severity(Severity::Error));
    assert_eq!(
        failing.len(),
        1,
        "only the finding missing from the baseline fails"
    );
    assert_eq!(failing[0].baseline, Some(bedrock_lint::BaselineState::New));
}

#[test]
fn candidates_and_waived_findings_never_fail() {
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/exe3_order");
    let (fw, _) = load(&fixture);
    let report = run(
        &fw,
        &RunOptions {
            only: vec!["EXE-3".into()],
            ..Default::default()
        },
    )
    .unwrap();
    let failing = report.failing(FailOn::Severity(Severity::Suggestion));
    assert_eq!(failing.len(), 3);
    assert!(failing.iter().all(|f| f.waived.is_none()));
    assert!(report.failing(FailOn::Never).is_empty());
    // a waiver without a reason fails the run on its own
    assert!(report.fails(FailOn::Never));
    assert!(failing.iter().any(|f| f.confidence == Confidence::Estimate));
}

#[test]
fn project_config_overrides_severity_and_skips() {
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/stk1_no_config");
    let (fw, _) = load(&fixture);
    let config: bedrock_lint::Config = toml::from_str(
        "[severity]\n\"STK-1\" = \"suggestion\"\n[skip]\n\"STK-4\" = \"bench checked\"\n",
    )
    .unwrap();
    let report = run(
        &fw,
        &RunOptions {
            config,
            ..Default::default()
        },
    )
    .unwrap();
    let stk1 = report.findings.iter().find(|f| f.check == "STK-1").unwrap();
    assert_eq!(stk1.severity, Severity::Suggestion);
    assert_eq!(report.skipped[0].check, "STK-4");
    assert!(!report.ran.iter().any(|r| r == "STK-4"));
}

#[test]
fn reports_render() {
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/exe3_order");
    let (fw, _) = load(&fixture);
    let report = run(&fw, &RunOptions::default()).unwrap();
    let text = bedrock_lint::report::text::render(
        &report,
        Some(&fw),
        FailOn::Severity(Severity::Warning),
        false,
    );
    assert!(text.contains("error[EXE-3]"), "{text}");
    assert!(
        text.contains("let _late = EX_LATE.start(interrupt::SWI_IRQ_0);"),
        "{text}"
    );
    assert!(text.contains("Waived (1):"), "{text}");
    assert!(text.contains("Waivers written wrong:"), "{text}");
    assert!(
        text.contains("Not run:") && text.contains("STK-2"),
        "{text}"
    );

    let sarif: serde_json::Value =
        serde_json::from_str(&bedrock_lint::report::sarif::render(&report)).unwrap();
    let run = &sarif["runs"][0];
    let results = run["results"].as_array().unwrap();
    assert_eq!(results.len(), report.findings.len());
    let rules = run["tool"]["driver"]["rules"].as_array().unwrap();
    for r in results {
        let idx = r["ruleIndex"].as_u64().unwrap() as usize;
        assert_eq!(rules[idx]["id"], r["ruleId"]);
    }
    assert!(
        results
            .iter()
            .any(|r| r["suppressions"][0]["kind"] == "inSource")
    );

    let json: serde_json::Value = serde_json::from_str(&report.to_json()).unwrap();
    assert_eq!(json["firmware"], "exe3_order");
}
