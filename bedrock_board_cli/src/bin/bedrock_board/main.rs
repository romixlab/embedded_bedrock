//! `bedrock-board`: the board description (`board.toml`) on the command line. `check` holds a description against
//! the chip's data and the PCB's nets, for every revision and rework; the `bedrock_board_model` library does the
//! work, this reads the inputs and prints.
//!
//! Shell completion: `source <(COMPLETE=bash bedrock-board)`, `COMPLETE=zsh bedrock-board`,
//! `COMPLETE=fish bedrock-board | source`.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use anstream::{eprintln, print, println};
use anstyle::{AnsiColor, Color, Style};
use bedrock_board_model::{Description, Diagnostic, LoadError, McuPins, Severity, Source};
use clap::{CommandFactory, Parser, Subcommand, ValueEnum};
use clap_complete::CompleteEnv;
use clap_complete::engine::{ArgValueCandidates, CompletionCandidate};

mod inputs;

const VERSION: &str = concat!(
    env!("CARGO_PKG_VERSION"),
    " (",
    env!("GIT_SHA"),
    ", commit ",
    env!("BUILD_TIME"),
    ")"
);

const BOLD: Style = Style::new().bold();
const DIM: Style = Style::new().dimmed();
const RED: Style = Style::new()
    .fg_color(Some(Color::Ansi(AnsiColor::Red)))
    .bold();
const YELLOW: Style = Style::new()
    .fg_color(Some(Color::Ansi(AnsiColor::Yellow)))
    .bold();
const CYAN: Style = Style::new()
    .fg_color(Some(Color::Ansi(AnsiColor::Cyan)))
    .bold();
const BLUE: Style = Style::new()
    .fg_color(Some(Color::Ansi(AnsiColor::Blue)))
    .bold();
const GREEN: Style = Style::new()
    .fg_color(Some(Color::Ansi(AnsiColor::Green)))
    .bold();

#[derive(Parser)]
#[command(
    name = "bedrock-board",
    version = VERSION,
    about = "The board description (board.toml): check it against the chip and the PCB",
    long_about = "The board description (board.toml): one typed file per board that says which MCU pin does what, \
        for every revision and rework of the board. `check` holds it against the chip's data (stm32-data) and the \
        PCB's nets.",
    after_help = "Examples:
  bedrock-board check                                  board.toml in the current directory
  bedrock-board check boards/b135/board.toml           every revision and rework of B135
  bedrock-board check board.toml --format json         the findings as JSON, for tools
  bedrock-board check board.toml --chip-data ~/stm32-data/data/chips

Completion: source <(COMPLETE=bash bedrock-board)   (zsh, fish likewise)"
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
    /// The configurations and every finding with line and column
    Json,
}

#[derive(Clone, Copy, ValueEnum)]
enum FailLevel {
    Error,
    Warning,
    Never,
}

#[derive(Subcommand)]
enum Cmd {
    /// Check a description: pins used twice, pins and signals against the chip, the PCB's nets
    #[command(
        after_help = "Checks, on every revision, every revision with each of its reworks and with all of them:
  pin-twice, free-used, short-both    a pin used twice; a [free] pin in use; two joined pins both used
  pin-unknown, package-unknown        a pin the chip or its package does not have
  periph-unknown, signal-unknown      a peripheral or signal the chip does not have
  signal-pin                          a signal on a pin that cannot carry it (help names the pins that can)
  pcb-unconnected, pcb-pad, pcb-net   a used pin not connected, not on the MCU footprint, on another net than `net`
  pcb-shared, pcb-short               pins on one net without a [short]; a [short] whose pins are on different nets
  pcb-undescribed                     a connected pin the description does not name (warning)

Chip data is stm32-data's chip JSON (<CHIP>.json), from --chip-data, BEDROCK_CHIP_DATA or the firmware
template generator's cache (~/.cache/bedrock_gen/chips; `bedrock_gen.py chip-info --chip CHIP` fills it).
Without it the chip checks are skipped, with a note.

Exit status: 0 nothing at --fail-on or worse, 1 findings that fail, 2 the description could not be read."
    )]
    Check {
        /// The description
        #[arg(default_value = "board.toml", add = ArgValueCandidates::new(description_candidates))]
        description: PathBuf,
        #[arg(long, short, value_enum, default_value_t = Format::Text)]
        format: Format,
        /// Findings at this severity or worse fail the run
        #[arg(long, value_enum, default_value_t = FailLevel::Warning)]
        fail_on: FailLevel,
        /// Directory of stm32-data chip JSON files
        #[arg(long, env = "BEDROCK_CHIP_DATA", value_name = "DIR")]
        chip_data: Option<PathBuf>,
    },
}

/// `board.toml` files below the current directory, with the board's name.
fn description_candidates() -> Vec<CompletionCandidate> {
    inputs::find_descriptions(Path::new("."), 3)
        .into_iter()
        .map(|(path, name)| {
            CompletionCandidate::new(path.display().to_string()).help(name.map(Into::into))
        })
        .collect()
}

/// What `main` turns into an exit status.
enum Outcome {
    Pass,
    Fail,
}

fn run(cli: Cli) -> anyhow::Result<Outcome> {
    match cli.color {
        ColorWhen::Always => anstream::ColorChoice::Always,
        ColorWhen::Never => anstream::ColorChoice::Never,
        ColorWhen::Auto => anstream::ColorChoice::Auto,
    }
    .write_global();
    match cli.command {
        Cmd::Check {
            description,
            format,
            fail_on,
            chip_data,
        } => check(&description, format, fail_on, chip_data),
    }
}

fn check(
    path: &Path,
    format: Format,
    fail_on: FailLevel,
    chip_data: Option<PathBuf>,
) -> anyhow::Result<Outcome> {
    let desc = match Description::load(path) {
        Ok(d) => d,
        Err(LoadError::Invalid { file, diagnostics }) => {
            print_report(&file, format, None, &[], &diagnostics);
            return Ok(Outcome::Fail);
        }
        Err(e @ LoadError::Read { .. }) => return Err(e.into()),
    };
    let mut notes = Vec::new();
    let chip = inputs::chip_data(&desc, chip_data.as_deref(), &mut notes);
    let pcbs: BTreeMap<String, McuPins> = inputs::pcbs(&desc, chip.as_ref(), &mut notes);
    let mut report = bedrock_board_model::check(&desc, chip.as_ref(), &pcbs);
    report.diagnostics.splice(0..0, notes);
    print_report(
        &desc.source,
        format,
        Some(&desc),
        &report.configs,
        &report.diagnostics,
    );
    let fails = match fail_on {
        FailLevel::Error => report.worst() >= Some(Severity::Error),
        FailLevel::Warning => report.worst() >= Some(Severity::Warning),
        FailLevel::Never => false,
    };
    Ok(if fails { Outcome::Fail } else { Outcome::Pass })
}

fn print_report(
    source: &Source,
    format: Format,
    desc: Option<&Description>,
    configs: &[String],
    diags: &[Diagnostic],
) {
    match format {
        Format::Json => println!("{}", json(source, desc, configs, diags)),
        Format::Text => {
            for d in diags {
                print!("{}", render(source, d, configs.len()));
            }
            let count = |s| diags.iter().filter(|d| d.severity == s).count();
            let (errors, warnings) = (count(Severity::Error), count(Severity::Warning));
            let title = desc.map_or_else(
                || source.path.display().to_string(),
                |d| format!("{} {}", d.board.name, d.board.chip.value),
            );
            let status = match (errors, warnings) {
                (0, 0) => format!("{GREEN}clean{GREEN:#}"),
                _ => format!(
                    "{RED}{errors} error(s){RED:#}, {YELLOW}{warnings} warning(s){YELLOW:#}"
                ),
            };
            let configs = if configs.is_empty()
                || desc.is_some_and(|d| d.board.revisions.is_empty() && configs.len() == 1)
            {
                String::new()
            } else {
                format!(" {DIM}({}){DIM:#}", configs.join(", "))
            };
            println!("{BOLD}{title}{BOLD:#}{configs}: {status}");
        }
    }
}

/// One diagnostic the way rustc prints one.
fn render(source: &Source, d: &Diagnostic, config_count: usize) -> String {
    let (style, word) = match d.severity {
        Severity::Error => (RED, "error"),
        Severity::Warning => (YELLOW, "warning"),
        Severity::Note => (CYAN, "note"),
    };
    let mut out = format!(
        "{style}{word}[{}]{style:#}{BOLD}: {}{BOLD:#}\n",
        d.code, d.message
    );
    let file = source.path.display();
    match &d.span {
        Some(span) => {
            let (line, col) = source.line_col(span.start);
            let text = source.line_text(line);
            let width = line.to_string().len();
            let len = source.text[span.clone()]
                .lines()
                .next()
                .map_or(1, |l| l.chars().count().max(1));
            let pad = " ".repeat(width);
            out += &format!("{pad}{BLUE}-->{BLUE:#} {file}:{line}:{col}\n");
            out += &format!("{pad} {BLUE}|{BLUE:#}\n");
            out += &format!("{BLUE}{line}{BLUE:#} {BLUE}|{BLUE:#} {text}\n");
            out += &format!(
                "{pad} {BLUE}|{BLUE:#} {}{style}{}{style:#}\n",
                " ".repeat(col - 1),
                "^".repeat(len)
            );
        }
        None => out += &format!(" {BLUE}-->{BLUE:#} {file}\n"),
    }
    if !d.configs.is_empty() && d.configs.len() < config_count {
        out += &format!(
            "  {BLUE}={BLUE:#} {BOLD}in{BOLD:#}: {}\n",
            d.configs.join(", ")
        );
    }
    if let Some(help) = &d.help {
        out += &format!("  {BLUE}={BLUE:#} {BOLD}help{BOLD:#}: {help}\n");
    }
    out + "\n"
}

fn json(
    source: &Source,
    desc: Option<&Description>,
    configs: &[String],
    diags: &[Diagnostic],
) -> String {
    let count = |s| diags.iter().filter(|d| d.severity == s).count();
    let findings: Vec<serde_json::Value> = diags
        .iter()
        .map(|d| {
            let mut v = serde_json::to_value(d).unwrap_or_default();
            if let (Some(span), Some(obj)) = (&d.span, v.as_object_mut()) {
                let (line, column) = source.line_col(span.start);
                obj.insert("line".into(), line.into());
                obj.insert("column".into(), column.into());
            }
            v
        })
        .collect();
    serde_json::json!({
        "description": source.path,
        "board": desc.map(|d| d.board.name.clone()),
        "chip": desc.map(|d| d.board.chip.value.clone()),
        "configurations": configs,
        "errors": count(Severity::Error),
        "warnings": count(Severity::Warning),
        "notes": count(Severity::Note),
        "diagnostics": findings,
    })
    .to_string()
}

fn main() -> ExitCode {
    CompleteEnv::with_factory(Cli::command).complete();
    match run(Cli::parse()) {
        Ok(Outcome::Pass) => ExitCode::SUCCESS,
        Ok(Outcome::Fail) => ExitCode::from(1),
        Err(e) => {
            eprintln!("{RED}error{RED:#}: {e:#}");
            ExitCode::from(2)
        }
    }
}
