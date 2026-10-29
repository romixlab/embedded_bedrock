//! `boardmap`: board infographics from a PCB file and a TOML description (the `bedrock_board` library does the
//! work; this parses arguments and prints).
//!
//! Shell completion: `source <(COMPLETE=bash boardmap)`, `COMPLETE=zsh boardmap`, `COMPLETE=fish boardmap | source`.

use std::path::PathBuf;
use std::process::ExitCode;

use anstream::println;
use anstyle::{AnsiColor, Color, Style};
use bedrock_board::Geometry;
use clap::{CommandFactory, Parser, Subcommand};
use clap_complete::CompleteEnv;

const BOLD: Style = Style::new().bold();
const DIM: Style = Style::new().dimmed();
const RED: Style = Style::new().fg_color(Some(Color::Ansi(AnsiColor::Red)));

/// Board infographics: top and bottom views, power tree and pin table of a dev board.
#[derive(Parser)]
#[command(
    name = "boardmap",
    version = env!("BOARDMAP_VERSION"),
    after_help = "Shell completion: source <(COMPLETE=bash boardmap), COMPLETE=zsh boardmap, COMPLETE=fish boardmap | source"
)]
struct Cli {
    #[command(subcommand)]
    command: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Read a .kicad_pcb once into the small geometry JSON a description points to
    Extract {
        /// The board file (KiCad; a vendor's Altium or Allegro board goes through `kicad-cli pcb import` first)
        pcb: PathBuf,
        /// Where to write the geometry JSON
        #[arg(short, long)]
        out: PathBuf,
        /// Keep only footprints whose reference starts with one of these (default: CN, J, JP, SB, LD, D, B, SW, U, X, Y)
        #[arg(long, value_delimiter = ',')]
        prefixes: Option<Vec<String>>,
        /// Where the file came from, printed in the pictures' footer
        #[arg(long, default_value = "the board's PCB file")]
        source: String,
        /// Rename pin names a symbol gives the MCU's pads, REGEX=REPLACEMENT (whole names, `$1` the first group;
        /// repeatable): `--pin-names '^GPIO(\d+)(_\d+)?$=GP$1'` turns `GPIO38_47` into `GP38`
        #[arg(long, value_name = "REGEX=REPLACEMENT")]
        pin_names: Vec<String>,
    },
    /// Render the pictures and the pin table from a description
    Render {
        /// The board description (TOML); its `geometry` entry names the JSON next to it
        desc: PathBuf,
        /// Output directory (default: the description's)
        #[arg(short, long)]
        out: Option<PathBuf>,
        /// Also write PNGs, at this scale of the SVG size
        #[arg(long, value_name = "SCALE")]
        png: Option<f32>,
    },
    /// List the footprints of a geometry JSON with their positions, to write a description from
    Parts { geometry: PathBuf },
}

fn run(cli: Cli) -> anyhow::Result<()> {
    match cli.command {
        Cmd::Extract {
            pcb,
            out,
            prefixes,
            source,
            pin_names,
        } => {
            let prefixes = prefixes.unwrap_or_else(|| {
                ["CN", "J", "JP", "SB", "LD", "D", "B", "SW", "U", "X", "Y"]
                    .into_iter()
                    .map(String::from)
                    .collect()
            });
            let mut geo = Geometry::extract(&pcb, &source, |reference| {
                prefixes.iter().any(|p| {
                    reference.starts_with(p.as_str())
                        && reference[p.len()..]
                            .chars()
                            .next()
                            .is_some_and(|c| c.is_ascii_digit())
                })
            })?;
            for rule in &pin_names {
                let (re, with) = rule
                    .rsplit_once('=')
                    .ok_or_else(|| anyhow::anyhow!("--pin-names {rule}: REGEX=REPLACEMENT"))?;
                let n = geo.rename_pins(re, with)?;
                println!("{DIM}--pin-names {rule}: {n} pins{DIM:#}");
            }
            geo.save(&out)?;
            println!(
                "{BOLD}{}{BOLD:#} footprints, {} outline segments, board {:.1} x {:.1} mm -> {}",
                geo.footprints.len(),
                geo.outline.len(),
                geo.bbox[2] - geo.bbox[0],
                geo.bbox[3] - geo.bbox[1],
                out.display()
            );
        }
        Cmd::Render { desc, out, png } => {
            let out_dir = out.unwrap_or_else(|| {
                desc.parent()
                    .map(PathBuf::from)
                    .unwrap_or_else(|| PathBuf::from("."))
            });
            let outputs = bedrock_board::render_all(&desc, &out_dir, png)?;
            for f in outputs.files {
                println!("{DIM}wrote{DIM:#} {}", f.display());
            }
        }
        Cmd::Parts { geometry } => {
            let geo = Geometry::load(&geometry)?;
            for (reference, fp) in &geo.footprints {
                println!(
                    "{BOLD}{reference:6}{BOLD:#} {:?} x={:7.2} y={:7.2} rot={:5.0} pads={:3} {DIM}{}{DIM:#}",
                    fp.side,
                    fp.at[0],
                    fp.at[1],
                    fp.rot,
                    fp.pads.len(),
                    fp.value
                );
            }
        }
    }
    Ok(())
}

fn main() -> ExitCode {
    CompleteEnv::with_factory(Cli::command).complete();
    match run(Cli::parse()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            anstream::eprintln!("{RED}error{RED:#}: {e:#}");
            ExitCode::FAILURE
        }
    }
}
