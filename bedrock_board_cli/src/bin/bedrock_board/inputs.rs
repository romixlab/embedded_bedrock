//! What `check` reads besides the description: the chip's data and the MCU's nets of each PCB.

use std::collections::BTreeMap;
use std::io::Read;
use std::path::{Path, PathBuf};

use bedrock_board::Geometry;
use bedrock_board::nets::{is_unconnected, pin_name};
use bedrock_board_model::{ChipData, Description, Diagnostic, Located, McuPins, Severity};

/// The largest chip JSON read; stm32-data's biggest chip files are under 1 MiB.
const CHIP_JSON_BYTES_MAX: u64 = 16 << 20;

/// A note that a check could not run.
fn note(
    code: &'static str,
    message: String,
    at: Option<&Located<String>>,
    help: Option<String>,
) -> Diagnostic {
    Diagnostic {
        code,
        severity: Severity::Note,
        message,
        span: at.map(|a| a.span.clone()),
        help,
        configs: Vec::new(),
    }
}

fn error(
    code: &'static str,
    message: String,
    at: &Located<String>,
    help: Option<String>,
) -> Diagnostic {
    Diagnostic {
        severity: Severity::Error,
        ..note(code, message, Some(at), help)
    }
}

/// The generator's cache of stm32-data chip files: `$XDG_CACHE_HOME` or `~/.cache`, then `bedrock_gen/chips`.
fn default_chip_dir() -> Option<PathBuf> {
    let cache = std::env::var_os("XDG_CACHE_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".cache")))?;
    Some(cache.join("bedrock_gen").join("chips"))
}

/// The chip's data, or a note why the chip checks are skipped.
pub(crate) fn chip_data(
    desc: &Description,
    dir: Option<&Path>,
    notes: &mut Vec<Diagnostic>,
) -> Option<ChipData> {
    let chip = &desc.board.chip;
    if !chip.value.to_ascii_uppercase().starts_with("STM32") {
        notes.push(note(
            "chip-data",
            format!(
                "no pin data for {} yet: pin and signal checks skipped",
                chip.value
            ),
            Some(chip),
            Some("chip data covers STM32 (stm32-data); RP2350 is planned (BOARD-5)".to_owned()),
        ));
        return None;
    }
    let Some(dir) = dir.map(Path::to_path_buf).or_else(default_chip_dir) else {
        notes.push(note(
            "chip-data",
            "no chip data directory: pin and signal checks skipped".to_owned(),
            Some(chip),
            None,
        ));
        return None;
    };
    let path = dir.join(format!("{}.json", chip.value.to_ascii_uppercase()));
    let text = match read_bounded(&path, CHIP_JSON_BYTES_MAX) {
        Ok(t) => t,
        Err(e) => {
            notes.push(note(
                "chip-data",
                format!("{}: {e}: pin and signal checks skipped", path.display()),
                Some(chip),
                Some(format!(
                    "fill it with `bedrock_gen.py chip-info --chip {}` or point --chip-data at stm32-data's data/chips",
                    chip.value
                )),
            ));
            return None;
        }
    };
    match ChipData::from_stm32_data(&text) {
        Ok(c) => Some(c),
        Err(e) => {
            notes.push(error(
                "chip-data",
                format!("{}: {e}", path.display()),
                chip,
                None,
            ));
            None
        }
    }
}

fn read_bounded(path: &Path, max_bytes: u64) -> std::io::Result<String> {
    let mut text = String::new();
    std::fs::File::open(path)?
        .take(max_bytes + 1)
        .read_to_string(&mut text)?;
    if text.len() as u64 > max_bytes {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            format!("longer than {max_bytes} bytes"),
        ));
    }
    Ok(text)
}

/// The MCU's nets of every PCB the configurations name, by the `pcb` value as written. A PCB that cannot be read
/// gets a diagnostic and no entry, so its configurations skip the PCB checks.
pub(crate) fn pcbs(
    desc: &Description,
    chip: Option<&ChipData>,
    out: &mut Vec<Diagnostic>,
) -> BTreeMap<String, McuPins> {
    let mut pcbs = BTreeMap::new();
    let Some(mcu) = &desc.board.mcu else {
        return pcbs;
    };
    // a pin name as the symbol gives it (`PC14-OSC32_IN`), cut to the GPIO the chip data knows
    let gpio_re = regex_lite::Regex::new(r"^P[A-Z]\d+(_C)?$").ok();
    for board in desc.configurations() {
        let Some(pcb) = &board.pcb else { continue };
        if pcbs.contains_key(&pcb.value) || out.iter().any(|d| d.span.as_ref() == Some(&pcb.span)) {
            continue;
        }
        let path = desc.dir().join(&pcb.value);
        if !path.exists() {
            out.push(note(
                "pcb-missing",
                format!(
                    "{} is not there: PCB checks skipped for {}",
                    path.display(),
                    board.label
                ),
                Some(pcb),
                None,
            ));
            continue;
        }
        let geo = if path.extension().is_some_and(|e| e == "kicad_pcb") {
            Geometry::extract(&path, &pcb.value, |r| r == mcu.value)
        } else {
            Geometry::load(&path)
        };
        let geo = match geo {
            Ok(g) => g,
            Err(e) => {
                out.push(error(
                    "pcb-read",
                    format!("{}: {e:#}", path.display()),
                    pcb,
                    None,
                ));
                continue;
            }
        };
        let Some(fp) = geo.footprints.get(&mcu.value) else {
            out.push(error(
                "pcb-mcu",
                format!("{} has no footprint {}", path.display(), mcu.value),
                mcu,
                Some(format!(
                    "footprints: {}",
                    geo.footprints
                        .keys()
                        .cloned()
                        .collect::<Vec<_>>()
                        .join(", ")
                )),
            ));
            continue;
        };
        let mut nets = BTreeMap::new();
        for pad in &fp.pads {
            let Some(name) = pad.pin.as_deref() else {
                continue;
            };
            let gpio = match chip {
                Some(c) => [Some(name), name.split_once('-').map(|(p, _)| p)]
                    .into_iter()
                    .flatten()
                    .find(|p| c.gpios.contains(*p)),
                None => gpio_re.as_ref().and_then(|re| pin_name(name, re)),
            };
            if let Some(gpio) = gpio {
                let net = pad
                    .net
                    .clone()
                    .filter(|n| !n.is_empty() && !is_unconnected(n));
                nets.insert(gpio.to_owned(), net);
            }
        }
        if nets.is_empty() {
            out.push(error(
                "pcb-mcu",
                format!(
                    "{} on {}: no pad carries a GPIO name",
                    mcu.value,
                    path.display()
                ),
                mcu,
                Some(
                    "the symbol's pin names give the GPIO; an imported vendor board has none"
                        .to_owned(),
                ),
            ));
            continue;
        }
        pcbs.insert(
            pcb.value.clone(),
            McuPins {
                reference: mcu.value.clone(),
                nets,
            },
        );
    }
    pcbs
}

/// `board.toml` files at most `depth` directories below `root`, with the board's name when it parses.
pub(crate) fn find_descriptions(root: &Path, depth: usize) -> Vec<(PathBuf, Option<String>)> {
    let mut out = Vec::new();
    let mut stack = vec![(root.to_path_buf(), 0)];
    while let Some((dir, level)) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let name = entry.file_name();
            if path.is_dir()
                && level < depth
                && !name.to_string_lossy().starts_with('.')
                && name != "target"
            {
                stack.push((path, level + 1));
            } else if name == "board.toml" {
                let board = Description::load(&path)
                    .ok()
                    .map(|d| format!("{} {}", d.board.name, d.board.chip.value));
                out.push((
                    path.strip_prefix(root).unwrap_or(&path).to_path_buf(),
                    board,
                ));
            }
        }
    }
    out.sort();
    out
}
