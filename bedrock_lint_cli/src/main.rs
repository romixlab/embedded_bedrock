//! `bedrock-lint`: checks a bare-metal Rust firmware crate against the firmware check catalogue
//! (embedded_bedrock `book/checks.md`). A thin CLI over the `bedrock_lint` library.

use anstream::{eprintln, print, println};
use bedrock_lint::catalogue::{catalogue, detail};
use bedrock_lint::report::{sarif, text};
use bedrock_lint::{Baseline, Config, FailOn, Firmware, LoadOptions, RunOptions, Severity};
use clap::{Parser, Subcommand, ValueEnum};
use clap_complete::CompleteEnv;
use clap_complete::engine::{ArgValueCandidates, CompletionCandidate};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

const VERSION: &str = concat!(
    env!("CARGO_PKG_VERSION"),
    " (",
    env!("GIT_SHA"),
    ", commit ",
    env!("BUILD_TIME"),
    ")"
);

#[derive(Parser)]
#[command(
    name = "bedrock-lint",
    version = VERSION,
    about = "Check a bare-metal Rust firmware crate against the firmware check catalogue",
    long_about = "Check a bare-metal Rust firmware crate against the firmware check catalogue \
        (embedded_bedrock book/checks.md): reads the crate's source, Cargo.toml, .cargo/config.toml, build.rs, \
        memory.x and bedrock_fw.json into a model and runs the automatic checks on it.",
    after_help = "Examples:
  bedrock-lint check                      check the firmware crate in the current directory
  bedrock-lint check fw/donguru --format sarif -o lint.sarif
  bedrock-lint check --write-baseline bedrock-lint.baseline.json   accept today's findings
  bedrock-lint explain EXE-3              what EXE-3 is about and how to fix it
  bedrock-lint checks list --all          every catalogue check and which are automated
  bedrock-lint check --elf target/thumbv6m-none-eabi/release/fw   with the elf checks (stack, sizes, libcalls)
  bedrock-lint frames target/thumbv6m-none-eabi/release/fw        the largest stack frames, the deepest path

Completion: source <(COMPLETE=bash bedrock-lint)   (zsh, fish likewise)"
)]
struct Cli {
    /// Colour: auto (a terminal, no NO_COLOR), always, never
    #[arg(long, value_enum, default_value_t = ColorWhen::Auto, global = true)]
    color: ColorWhen,
    #[command(subcommand)]
    command: Cmd,
}

#[derive(Clone, Copy, ValueEnum)]
enum ColorWhen {
    Auto,
    Always,
    Never,
}

#[derive(Clone, Copy, PartialEq, Eq, ValueEnum)]
enum Format {
    /// For a terminal, like rustc's diagnostics
    Text,
    /// The whole report (findings, not run, waivers, notes)
    Json,
    /// SARIF 2.1.0, for code-scanning viewers
    Sarif,
}

#[derive(Clone, Copy, ValueEnum)]
enum FailLevel {
    Error,
    Warning,
    Suggestion,
    Never,
}

#[derive(Subcommand)]
enum Cmd {
    /// Run the checks on a firmware crate
    #[command(
        after_help = "Exit status: 0 nothing fails, 1 findings at --fail-on or worse (not waived, not \
        candidates, not in the baseline) or a waiver written wrong, 2 the crate could not be read.

Waivers sit in the firmware's files, with a reason:
  // bedrock-lint: allow(EXE-3) the ROM leaves this vector at P2      (Rust, above the item or at the line's end)
  # bedrock-lint: allow(STK-4) checked on the bench                   (TOML)
bedrock-lint.toml in the crate sets severities, skips checks with a reason and names the baseline."
    )]
    Check {
        /// The firmware crate's directory
        #[arg(default_value = ".")]
        dir: PathBuf,
        #[arg(long, short, value_enum, default_value_t = Format::Text)]
        format: Format,
        /// Write the report here as well (in --format); the text report still goes to the terminal
        #[arg(long, short)]
        output: Option<PathBuf>,
        /// Only these checks or categories (EXE-3, STK); repeat or separate with commas
        #[arg(long, value_delimiter = ',', add = ArgValueCandidates::new(check_candidates))]
        only: Vec<String>,
        /// Findings at this severity or worse fail the run
        #[arg(long, value_enum, default_value_t = FailLevel::Warning)]
        fail_on: FailLevel,
        /// Compare with this baseline: only new findings fail (default: `baseline` of bedrock-lint.toml)
        #[arg(long)]
        baseline: Option<PathBuf>,
        /// Write the findings of this run (not waived) as the baseline and exit 0
        #[arg(long)]
        write_baseline: Option<PathBuf>,
        /// The linked ELF, for the elf checks
        #[arg(long)]
        elf: Option<PathBuf>,
        /// Where discovery of .cargo/config.toml, the workspace and path dependencies stops (default: the
        /// repository root, the nearest parent with a .git)
        #[arg(long)]
        root: Option<PathBuf>,
    },
    /// What a check is about: its catalogue line and detail page
    Explain {
        /// A check id (EXE-3) or a category (EXE)
        #[arg(add = ArgValueCandidates::new(check_candidates))]
        id: String,
    },
    /// The checks of the catalogue
    Checks {
        #[command(subcommand)]
        command: ChecksCmd,
    },
    /// The largest stack frames of a linked ELF and the deepest call path (Arm Thumb code)
    #[command(
        after_help = "Frames come from each function's prologue, the paths from its direct calls (bl, tail calls); \
        calls through pointers (async task polls, dyn methods) are not followed, so a path can end early."
    )]
    Frames {
        /// The linked ELF (target/<triple>/release/<name>)
        elf: PathBuf,
        /// How many functions to list
        #[arg(long, default_value_t = 20)]
        top: usize,
        /// The deepest path from this function (default: Reset)
        #[arg(long)]
        from: Option<String>,
    },
    /// Code bytes per source file (or crate) of a linked ELF, from its DWARF line table
    #[command(
        after_help = "Each line table row counts for the file of the innermost inlined function, so code inlined \
        across crates by LTO is still split by where it was written. Needs debug info (debug = 2, not stripped)."
    )]
    CodeSize {
        /// The linked ELF (target/<triple>/release/<name>)
        elf: PathBuf,
        /// Sum by crate instead of by file
        #[arg(long)]
        by_crate: bool,
        /// Only files (or crates) whose path contains this; repeat for several
        #[arg(long = "match", short)]
        matches: Vec<String>,
        /// How many rows to list (0: all)
        #[arg(long, default_value_t = 0)]
        top: usize,
    },
    /// Print the firmware model as JSON: files read, chip, source facts (tasks, executors, spawns, ...)
    Model {
        #[arg(default_value = ".")]
        dir: PathBuf,
        #[arg(long)]
        elf: Option<PathBuf>,
        #[arg(long)]
        root: Option<PathBuf>,
    },
}

#[derive(Subcommand)]
enum ChecksCmd {
    /// The automated checks; --all for every check of the catalogue
    List {
        /// Every check of the catalogue, with how each is checked
        #[arg(long)]
        all: bool,
        /// Only this category (EXE)
        #[arg(long, add = ArgValueCandidates::new(category_candidates))]
        category: Option<String>,
        #[arg(long)]
        json: bool,
    },
}

fn check_candidates() -> Vec<CompletionCandidate> {
    let cat = catalogue();
    cat.categories
        .iter()
        .map(|c| CompletionCandidate::new(&c.id).help(Some(c.title.clone().into())))
        .chain(
            cat.entries
                .iter()
                .map(|e| CompletionCandidate::new(&e.id).help(Some(e.plain_title().into()))),
        )
        .collect()
}

fn category_candidates() -> Vec<CompletionCandidate> {
    catalogue()
        .categories
        .iter()
        .map(|c| CompletionCandidate::new(&c.id).help(Some(c.title.clone().into())))
        .collect()
}

fn main() -> ExitCode {
    CompleteEnv::with_factory(<Cli as clap::CommandFactory>::command).complete();
    let cli = Cli::parse();
    let choice = match cli.color {
        ColorWhen::Always => anstream::ColorChoice::Always,
        ColorWhen::Never => anstream::ColorChoice::Never,
        ColorWhen::Auto => anstream::ColorChoice::Auto,
    };
    choice.write_global();
    let result = match cli.command {
        Cmd::Check {
            dir,
            format,
            output,
            only,
            fail_on,
            baseline,
            write_baseline,
            elf,
            root,
        } => check(CheckArgs {
            dir,
            format,
            output,
            only,
            fail_on,
            baseline,
            write_baseline,
            elf,
            root,
        }),
        Cmd::Explain { id } => explain(&id),
        Cmd::Checks {
            command:
                ChecksCmd::List {
                    all,
                    category,
                    json,
                },
        } => list(all, category.as_deref(), json),
        Cmd::Model { dir, elf, root } => model(&dir, elf, root),
        Cmd::Frames { elf, top, from } => frames(&elf, top, from.as_deref()),
        Cmd::CodeSize {
            elf,
            by_crate,
            matches,
            top,
        } => code_size(&elf, by_crate, &matches, top),
    };
    match result {
        Ok(code) => code,
        Err(e) => {
            let red = anstyle::Style::new()
                .fg_color(Some(anstyle::AnsiColor::Red.into()))
                .bold();
            eprintln!("{red}error{red:#}: {e}");
            ExitCode::from(2)
        }
    }
}

struct CheckArgs {
    dir: PathBuf,
    format: Format,
    output: Option<PathBuf>,
    only: Vec<String>,
    fail_on: FailLevel,
    baseline: Option<PathBuf>,
    write_baseline: Option<PathBuf>,
    elf: Option<PathBuf>,
    root: Option<PathBuf>,
}

fn load(dir: &Path, elf: Option<PathBuf>, root: Option<PathBuf>) -> Result<Firmware, String> {
    Firmware::load(dir, &LoadOptions { root, elf }).map_err(|e| e.to_string())
}

fn check(a: CheckArgs) -> Result<ExitCode, String> {
    let fw = load(&a.dir, a.elf, a.root)?;
    let config = Config::load(&fw.dir)?;
    let baseline_path = a
        .baseline
        .clone()
        .or_else(|| config.baseline.as_ref().map(|b| fw.dir.join(b)));
    let baseline = match (&baseline_path, &a.write_baseline) {
        // writing a new baseline: compare with nothing
        (_, Some(_)) => None,
        (Some(p), None) if p.is_file() || a.baseline.is_some() => Some(Baseline::load(p)?),
        _ => None,
    };
    let report = bedrock_lint::run(
        &fw,
        &RunOptions {
            only: a.only,
            config,
            baseline,
        },
    )
    .map_err(|e| e.to_string())?;
    let fail_on = match a.fail_on {
        FailLevel::Error => FailOn::Severity(Severity::Error),
        FailLevel::Warning => FailOn::Severity(Severity::Warning),
        FailLevel::Suggestion => FailOn::Severity(Severity::Suggestion),
        FailLevel::Never => FailOn::Never,
    };

    if let Some(p) = &a.write_baseline {
        let b = Baseline::from_findings(&report.findings);
        std::fs::write(p, b.to_json()).map_err(|e| format!("{}: {e}", p.display()))?;
        println!(
            "{}: wrote {} finding(s) to {}",
            report.firmware,
            b.findings.len(),
            p.display()
        );
        return Ok(ExitCode::SUCCESS);
    }

    let rendered = match a.format {
        Format::Text => text::render(&report, Some(&fw), fail_on, false),
        Format::Json => report.to_json(),
        Format::Sarif => sarif::render(&report),
    };
    if let Some(p) = &a.output {
        std::fs::write(p, &rendered).map_err(|e| format!("{}: {e}", p.display()))?;
    }
    if a.output.is_none() && a.format != Format::Text {
        // machine output on stdout, nothing else there
        std::io::Write::write_all(&mut std::io::stdout(), rendered.as_bytes())
            .map_err(|e| e.to_string())?;
    } else {
        print!("{}", text::render(&report, Some(&fw), fail_on, true));
    }
    Ok(if report.fails(fail_on) {
        ExitCode::from(1)
    } else {
        ExitCode::SUCCESS
    })
}

fn explain(id: &str) -> Result<ExitCode, String> {
    let cat = catalogue();
    let bold = anstyle::Style::new().bold();
    let dim = anstyle::Style::new().dimmed();
    if let Some(c) = cat.category(id) {
        println!("{bold}{} - {}{bold:#} ({})", c.id, c.title, c.group);
        for e in cat.in_category(&c.id) {
            println!("  {} {} {dim}· {}{dim:#}", e.id, e.plain_title(), e.check);
        }
        return Ok(ExitCode::SUCCESS);
    }
    let e = cat.entry(id).ok_or_else(|| {
        format!("no check or category {id} in the catalogue (bedrock-lint checks list --all)")
    })?;
    println!("{bold}{} {}{bold:#}", e.id, e.plain_title());
    println!("  severity: {}", e.severity);
    println!("  checked: {}", e.check);
    if let Some(chip) = &e.chip {
        println!("  chip: {chip}");
    }
    let automated = bedrock_lint::check::check(&e.id).is_some();
    println!(
        "  bedrock-lint: {}",
        if automated {
            "automated"
        } else if e.automatic() {
            "not automated yet"
        } else {
            "a human check (review, test or bench)"
        }
    );
    if let Some(s) = &e.source {
        println!("  source: {s}");
    }
    println!("  catalogue: {}", e.link());
    match detail(e) {
        Some(d) => println!("\n{d}"),
        None => println!(
            "  {dim}no detail page yet (book/checks/{}.md){dim:#}",
            e.page()
        ),
    }
    Ok(ExitCode::SUCCESS)
}

fn list(all: bool, category: Option<&str>, json: bool) -> Result<ExitCode, String> {
    let cat = catalogue();
    if let Some(c) = category
        && cat.category(c).is_none()
    {
        return Err(format!("no category {c} in the catalogue"));
    }
    let entries: Vec<_> = cat
        .entries
        .iter()
        .filter(|e| category.is_none_or(|c| e.category.eq_ignore_ascii_case(c)))
        .filter(|e| all || bedrock_lint::check::check(&e.id).is_some())
        .collect();
    if json {
        let v: Vec<serde_json::Value> = entries
            .iter()
            .map(|e| {
                serde_json::json!({
                    "id": e.id,
                    "severity": e.severity,
                    "title": e.title,
                    "check": e.check,
                    "automated": bedrock_lint::check::check(&e.id).is_some(),
                })
            })
            .collect();
        println!("{}", serde_json::to_string_pretty(&v).unwrap());
        return Ok(ExitCode::SUCCESS);
    }
    let green = anstyle::Style::new().fg_color(Some(anstyle::AnsiColor::Green.into()));
    let dim = anstyle::Style::new().dimmed();
    for e in &entries {
        let mark = if bedrock_lint::check::check(&e.id).is_some() {
            format!("{green}automated{green:#}")
        } else if e.automatic() {
            format!("{dim}not yet   {dim:#}")
        } else {
            format!("{dim}human     {dim:#}")
        };
        let sev = match e.severity {
            Severity::Error => "🛑",
            Severity::Warning => "⚠️ ",
            Severity::Suggestion => "💡",
        };
        println!(
            "{sev} {:<8} {mark}  {} {dim}· {}{dim:#}",
            e.id,
            e.plain_title(),
            e.check
        );
    }
    let automated = cat
        .entries
        .iter()
        .filter(|e| bedrock_lint::check::check(&e.id).is_some())
        .count();
    let automatic = cat.entries.iter().filter(|e| e.automatic()).count();
    println!(
        "{dim}{automated} automated of {automatic} automatic checks ({} in the catalogue){}{dim:#}",
        cat.entries.len(),
        if all { "" } else { "; --all lists every check" }
    );
    Ok(ExitCode::SUCCESS)
}

fn model(dir: &Path, elf: Option<PathBuf>, root: Option<PathBuf>) -> Result<ExitCode, String> {
    let fw = load(dir, elf, root)?;
    let json = serde_json::to_string_pretty(&fw).map_err(|e| e.to_string())?;
    std::io::Write::write_all(&mut std::io::stdout(), format!("{json}\n").as_bytes())
        .map_err(|e| e.to_string())?;
    Ok(ExitCode::SUCCESS)
}

fn frames(path: &Path, top: usize, from: Option<&str>) -> Result<ExitCode, String> {
    use bedrock_lint::model::elf::{CallDepths, Elf};
    let elf = Elf::load(path, Path::new(".")).map_err(|e| format!("{}: {e}", path.display()))?;
    if elf.code.is_empty() {
        return Err(format!(
            "{}: no Arm Thumb functions: frames are read from Thumb code only",
            path.display()
        ));
    }
    let bold = anstyle::Style::new().bold();
    let dim = anstyle::Style::new().dimmed();
    let mut d = CallDepths::new(&elf);
    let mut rows: Vec<(u32, u64, &str)> = Vec::new();
    for c in &elf.code {
        rows.push((
            c.frame.unwrap_or(0),
            d.depth(c.symbol),
            &elf.symbols[c.symbol].name,
        ));
    }
    rows.sort_by(|a, b| b.0.cmp(&a.0).then(b.1.cmp(&a.1)));
    println!("{bold}{:>7}  {:>7}  function{bold:#}", "frame", "deepest");
    for (frame, depth, name) in rows.iter().take(top) {
        println!("{frame:>7}  {depth:>7}  {name}");
    }
    let start = from.unwrap_or("Reset");
    let Some(root) = elf
        .symbols
        .iter()
        .position(|s| s.name == start || s.raw == start)
    else {
        return Err(format!("no function {start} in the ELF"));
    };
    let total = d.depth(root);
    println!();
    println!("{bold}deepest path from {start}: {total} bytes{bold:#}");
    for i in d.path(root) {
        let frame = elf.code_of(i).and_then(|c| c.frame).unwrap_or(0);
        println!("{frame:>7}  {}", elf.symbols[i].name);
    }
    let indirect: u32 = elf.code.iter().map(|c| c.indirect).sum();
    println!(
        "{dim}{} functions, {indirect} calls through pointers not followed{dim:#}",
        elf.code.len()
    );
    Ok(ExitCode::SUCCESS)
}

fn code_size(
    path: &Path,
    by_crate: bool,
    matches: &[String],
    top: usize,
) -> Result<ExitCode, String> {
    let data = std::fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let size = bedrock_lint::model::code_size::code_size(&data)
        .map_err(|e| format!("{}: {e}", path.display()))?;
    let bold = anstyle::Style::new().bold();
    let dim = anstyle::Style::new().dimmed();
    // files under the current directory without its prefix
    let here = std::env::current_dir()
        .map(|d| format!("{}/", d.display()))
        .unwrap_or_default();
    let all: std::collections::BTreeMap<String, u64> = if by_crate {
        size.by_crate()
    } else {
        size.by_file
            .iter()
            .map(|(k, v)| (k.strip_prefix(here.as_str()).unwrap_or(k).to_string(), *v))
            .collect()
    };
    let mut rows: Vec<(&String, u64)> = all
        .iter()
        .filter(|(k, _)| matches.is_empty() || matches.iter().any(|m| k.contains(m.as_str())))
        .map(|(k, v)| (k, *v))
        .collect();
    rows.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(b.0)));
    let sum: u64 = rows.iter().map(|r| r.1).sum();
    println!(
        "{bold}{:>8}  {}{bold:#}",
        "bytes",
        if by_crate { "crate" } else { "file" }
    );
    let shown = if top == 0 {
        rows.len()
    } else {
        top.min(rows.len())
    };
    for (k, v) in &rows[..shown] {
        println!("{v:>8}  {k}");
    }
    if shown < rows.len() {
        let rest: u64 = rows[shown..].iter().map(|r| r.1).sum();
        println!("{dim}{rest:>8}  {} more{dim:#}", rows.len() - shown);
    }
    println!("{bold}{sum:>8}  listed{bold:#}");
    println!(
        "{dim}{:>8}  attributed of {} in executable sections{dim:#}",
        size.attributed(),
        size.exec
    );
    Ok(ExitCode::SUCCESS)
}
