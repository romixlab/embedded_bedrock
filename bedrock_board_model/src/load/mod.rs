//! Reading `board.toml`: parse with spans, check its structure, resolve revisions and reworks into configurations.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::path::{Path, PathBuf};

use crate::schema::{
    Board, BoardInfo, Button, Entries, Gpio, GpioMode, Led, LedPins, Periph, PinRef, Reserved,
    Short,
};
use crate::source::{Diagnostic, Located, Source, Span};

mod raw;

/// A loaded description: the file, its `[board]`, the base entries and every configuration.
#[derive(Debug, Clone)]
pub struct Description {
    pub source: Source,
    pub board: BoardInfo,
    /// The entries outside `[rev.*]` and `[rework.*]`.
    pub base: Entries,
    configs: Vec<Board>,
}

/// Why a description could not be loaded.
#[derive(Debug)]
pub enum LoadError {
    Read {
        path: PathBuf,
        error: std::io::Error,
    },
    /// The file is not a valid description; every problem found, with its place in `file`.
    Invalid {
        file: Source,
        diagnostics: Vec<Diagnostic>,
    },
}

impl fmt::Display for LoadError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Read { path, .. } => write!(f, "reading {}", path.display()),
            Self::Invalid { file, diagnostics } => write!(
                f,
                "{}: {} problem(s) in the description",
                file.path.display(),
                diagnostics.len()
            ),
        }
    }
}

impl std::error::Error for LoadError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Read { error, .. } => Some(error),
            Self::Invalid { .. } => None,
        }
    }
}

impl Description {
    /// Reads and resolves a description.
    pub fn load(path: &Path) -> Result<Self, LoadError> {
        let text = read_bounded(path, DESCRIPTION_BYTES_MAX).map_err(|error| LoadError::Read {
            path: path.to_owned(),
            error,
        })?;
        Self::parse(path.to_owned(), text)
    }

    /// Resolves a description from its text; `path` names it in messages and anchors relative paths.
    pub fn parse(path: PathBuf, text: String) -> Result<Self, LoadError> {
        let source = Source { path, text };
        let mut diags = Vec::new();
        let mut file: raw::File = match toml::from_str(&source.text) {
            Ok(f) => f,
            Err(e) => {
                let mut d = Diagnostic::error("parse", e.message().to_owned());
                d.span = e.span();
                return Err(LoadError::Invalid {
                    file: source,
                    diagnostics: vec![d],
                });
            }
        };
        let base = entries(file.take_entries(), &mut diags);
        let board = board_info(file.board);
        let revisions = check_revisions(&board, &mut diags);
        let revs = patches(file.rev, &mut diags);
        let reworks = patches(file.rework, &mut diags);
        for (name, p) in &revs {
            if !revisions.contains(name.value.as_str()) {
                diags.push(
                    Diagnostic::error(
                        "rev-unknown",
                        format!(
                            "[rev.{}] names no revision of [board] revisions",
                            name.value
                        ),
                    )
                    .at(&name.span)
                    .help(format!("[board] revisions = {:?}", revisions)),
                );
            }
            if let Some(r) = &p.revisions {
                diags.push(
                    Diagnostic::error(
                        "rev-revisions",
                        "`revisions` belongs to a rework, not a revision",
                    )
                    .at(&r.span),
                );
            }
        }
        for (name, p) in &reworks {
            if let Some(pcb) = &p.pcb {
                diags.push(
                    Diagnostic::error("rework-pcb", format!("rework {} names a PCB: a rework changes the board, the PCB stays its revision's", name.value))
                        .at(&pcb.span),
                );
            }
            for r in p.revisions.iter().flat_map(|r| &r.value) {
                if !revisions.contains(r.value.as_str()) {
                    diags.push(
                        Diagnostic::error("rework-revision", format!("rework {} applies to revision {}, which [board] revisions does not list", name.value, r.value))
                            .at(&r.span),
                    );
                }
            }
        }
        if board.mcu.is_none()
            && let Some(pcb) = board
                .pcb
                .as_ref()
                .or(revs.iter().find_map(|(_, p)| p.pcb.as_ref()))
        {
            diags.push(
                Diagnostic::error("pcb-mcu", "a PCB is named but not the MCU's reference")
                    .at(&pcb.span)
                    .help("[board] mcu = \"U201\""),
            );
        }
        let configs = configurations(&board, &base, &revs, &reworks, &mut diags);
        let mut seen = BTreeSet::new();
        diags.retain(|d| {
            seen.insert((
                d.code,
                d.message.clone(),
                d.span.as_ref().map(|s| (s.start, s.end)),
            ))
        });
        if diags.is_empty() {
            Ok(Self {
                source,
                board,
                base,
                configs,
            })
        } else {
            Err(LoadError::Invalid {
                file: source,
                diagnostics: diags,
            })
        }
    }

    /// Every configuration: each revision; each revision with each rework that applies to it; each revision with
    /// all of them when more than one applies. A board without revisions is one configuration (and its reworks).
    pub fn configurations(&self) -> &[Board] {
        &self.configs
    }

    /// The directory relative paths of the description (`pcb`) start from.
    pub fn dir(&self) -> &Path {
        self.source.path.parent().unwrap_or(Path::new("."))
    }
}

/// The largest description read: a board with every pin described twice over is under 100 KiB.
pub const DESCRIPTION_BYTES_MAX: u64 = 4 << 20;

/// Reads a UTF-8 file of at most `max_bytes`; a longer one is an `InvalidData` error.
pub(crate) fn read_bounded(path: &Path, max_bytes: u64) -> std::io::Result<String> {
    use std::io::Read;
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

fn located(s: toml::Spanned<String>) -> Located<String> {
    Located::from_toml(s)
}

fn board_info(b: raw::Board) -> BoardInfo {
    BoardInfo {
        name: b.name,
        chip: located(b.chip),
        package: b.package.map(located),
        mcu: b.mcu.map(located),
        pcb: b.pcb.map(located),
        revisions: b.revisions.into_iter().map(located).collect(),
    }
}

fn check_revisions<'a>(board: &'a BoardInfo, diags: &mut Vec<Diagnostic>) -> BTreeSet<&'a str> {
    let mut seen = BTreeSet::new();
    for r in &board.revisions {
        if !seen.insert(r.value.as_str()) {
            diags.push(
                Diagnostic::error("rev-twice", format!("revision {} is listed twice", r.value))
                    .at(&r.span),
            );
        }
    }
    seen
}

/// A patch after its structure is checked.
struct Patch {
    note: Option<String>,
    pcb: Option<Located<String>>,
    revisions: Option<Located<Vec<Located<String>>>>,
    remove: Vec<Located<String>>,
    entries: Entries,
}

fn patches(
    raw: BTreeMap<toml::Spanned<String>, raw::Patch>,
    diags: &mut Vec<Diagnostic>,
) -> Vec<(Located<String>, Patch)> {
    raw.into_iter()
        .map(|(name, mut p)| {
            let entries = entries(p.take_entries(), diags);
            let revisions = p.revisions.map(|r| {
                let span = r.span();
                Located {
                    value: r.into_inner().into_iter().map(located).collect(),
                    span,
                }
            });
            let patch = Patch {
                note: p.note,
                pcb: p.pcb.map(located),
                revisions,
                remove: p.remove.into_iter().map(located).collect(),
                entries,
            };
            (located(name), patch)
        })
        .collect()
}

fn pin_ref(p: raw::PinRef) -> PinRef {
    let span = p.span();
    match p.into_inner() {
        raw::PinRefValue::Pin(pin) => PinRef {
            pin: Located { value: pin, span },
            net: None,
        },
        raw::PinRefValue::Table { pin, net } => PinRef {
            pin: located(pin),
            net: net.map(located),
        },
    }
}

fn entries(e: raw::Entries, diags: &mut Vec<Diagnostic>) -> Entries {
    let mut out = Entries::default();
    for (name, g) in e.gpio {
        if let Some(mode) = gpio_mode(&name, &g, diags) {
            out.gpio.insert(
                name,
                Gpio {
                    pin: pin_ref(g.pin),
                    mode,
                    note: g.note,
                },
            );
        }
    }
    for (name, p) in e.periph {
        out.periph.insert(
            name,
            Periph {
                instance: located(p.instance),
                pins: p.pins.into_iter().map(|(s, r)| (s, pin_ref(r))).collect(),
                note: p.note,
            },
        );
    }
    for (name, l) in e.led {
        let pins = match (l.pin, l.rgb) {
            (Some(pin), None) => Some(LedPins::Mono(pin_ref(pin))),
            (None, Some(rgb)) => {
                let span = rgb.span();
                match <[raw::PinRef; 3]>::try_from(rgb.into_inner()) {
                    Ok(pins) => Some(LedPins::Rgb(pins.map(pin_ref))),
                    Err(_) => {
                        diags.push(
                            Diagnostic::error(
                                "led-rgb",
                                format!("led.{name}: `rgb` takes three pins, red, green, blue"),
                            )
                            .at(&span),
                        );
                        None
                    }
                }
            }
            _ => {
                diags.push(Diagnostic::error(
                    "led-pins",
                    format!(
                        "led.{name}: give `pin` (one colour) or `rgb` (three pins), one of them"
                    ),
                ));
                None
            }
        };
        if let Some(pins) = pins {
            out.led.insert(
                name,
                Led {
                    pins,
                    active: l.active,
                    note: l.note,
                },
            );
        }
    }
    for (name, b) in e.button {
        out.button.insert(
            name,
            Button {
                pin: pin_ref(b.pin),
                active: b.active,
                pull: b.pull,
                note: b.note,
            },
        );
    }
    for (name, r) in e.reserved {
        out.reserved.insert(
            name,
            Reserved {
                pins: r.pins.into_iter().map(pin_ref).collect(),
                note: r.note,
            },
        );
    }
    for (name, s) in e.short {
        let span = s.pins.span();
        let pins: Vec<Located<String>> = s.pins.into_inner().into_iter().map(located).collect();
        let distinct: BTreeSet<&str> = pins.iter().map(|p| p.value.as_str()).collect();
        if distinct.len() < 2 || distinct.len() != pins.len() {
            diags.push(
                Diagnostic::error(
                    "short-pins",
                    format!("short.{name}: list two or more different pins"),
                )
                .at(&span),
            );
            continue;
        }
        out.short.insert(name, Short { pins, note: s.note });
    }
    for (pin, note) in e.free {
        out.free.insert(
            pin.get_ref().clone(),
            Located {
                value: note.get_ref().clone(),
                span: pin.span(),
            },
        );
    }
    out
}

fn gpio_mode(name: &str, g: &raw::Gpio, diags: &mut Vec<Diagnostic>) -> Option<GpioMode> {
    let mode_span = g.mode.span();
    let bad = |field: &str, span: Span, why: &str| {
        Diagnostic::error("gpio-fields", format!("gpio.{name}: `{field}` {why}")).at(&span)
    };
    match g.mode.get_ref() {
        raw::Mode::Output => {
            if let Some(p) = &g.pull {
                diags.push(bad("pull", p.span(), "is for an input"));
            }
            match &g.init {
                Some(init) => Some(GpioMode::Output {
                    init: *init.get_ref(),
                }),
                None => {
                    diags.push(bad(
                        "init",
                        mode_span,
                        "is missing: an output needs its level from reset (\"high\" or \"low\")",
                    ));
                    None
                }
            }
        }
        raw::Mode::Input => {
            if let Some(i) = &g.init {
                diags.push(bad("init", i.span(), "is for an output"));
            }
            match &g.pull {
                Some(pull) => Some(GpioMode::Input {
                    pull: *pull.get_ref(),
                }),
                None => {
                    diags.push(bad(
                        "pull",
                        mode_span,
                        "is missing: \"up\", \"down\" or \"none\" (the board holds the line)",
                    ));
                    None
                }
            }
        }
        raw::Mode::Analog => {
            for (field, span) in [
                ("init", g.init.as_ref().map(|s| s.span())),
                ("pull", g.pull.as_ref().map(|s| s.span())),
            ] {
                if let Some(span) = span {
                    diags.push(bad(field, span, "is not for an analog pin"));
                }
            }
            Some(GpioMode::Analog)
        }
    }
}

/// Applies a patch to `into`; the names of the entries it touches go to `touched`.
fn apply(
    into: &mut Entries,
    patch: &Patch,
    what: &str,
    touched: &mut BTreeSet<String>,
    diags: &mut Vec<Diagnostic>,
) {
    fn put<T: Clone>(
        into: &mut BTreeMap<String, T>,
        from: &BTreeMap<String, T>,
        section: &str,
        touched: &mut BTreeSet<String>,
    ) {
        for (k, v) in from {
            into.insert(k.clone(), v.clone());
            touched.insert(format!("{section}.{k}"));
        }
    }
    for r in &patch.remove {
        let removed = match r.value.split_once('.') {
            Some(("gpio", n)) => into.gpio.remove(n).is_some(),
            Some(("periph", n)) => into.periph.remove(n).is_some(),
            Some(("led", n)) => into.led.remove(n).is_some(),
            Some(("button", n)) => into.button.remove(n).is_some(),
            Some(("reserved", n)) => into.reserved.remove(n).is_some(),
            Some(("short", n)) => into.short.remove(n).is_some(),
            Some(("free", n)) => into.free.remove(n).is_some(),
            _ => {
                diags.push(
                    Diagnostic::error(
                        "remove-name",
                        format!("{what}: remove {:?}: expected SECTION.NAME", r.value),
                    )
                    .at(&r.span)
                    .help("sections: gpio, periph, led, button, reserved, short, free"),
                );
                continue;
            }
        };
        if removed {
            touched.insert(r.value.clone());
        } else {
            diags.push(
                Diagnostic::error(
                    "remove-unknown",
                    format!("{what}: removes {}, which is not there", r.value),
                )
                .at(&r.span),
            );
        }
    }
    let e = &patch.entries;
    put(&mut into.gpio, &e.gpio, "gpio", touched);
    put(&mut into.periph, &e.periph, "periph", touched);
    put(&mut into.led, &e.led, "led", touched);
    put(&mut into.button, &e.button, "button", touched);
    put(&mut into.reserved, &e.reserved, "reserved", touched);
    put(&mut into.short, &e.short, "short", touched);
    put(&mut into.free, &e.free, "free", touched);
}

fn configurations(
    board: &BoardInfo,
    base: &Entries,
    revs: &[(Located<String>, Patch)],
    reworks: &[(Located<String>, Patch)],
    diags: &mut Vec<Diagnostic>,
) -> Vec<Board> {
    let mut out = Vec::new();
    let revisions: Vec<Option<&str>> = if board.revisions.is_empty() {
        vec![None]
    } else {
        board
            .revisions
            .iter()
            .map(|r| Some(r.value.as_str()))
            .collect()
    };
    let mut overlaps_reported = BTreeSet::new();
    for rev in revisions {
        let mut entries = base.clone();
        let mut pcb = board.pcb.clone();
        let mut notes = Vec::new();
        let label = rev.map_or_else(|| board.name.clone(), |r| format!("rev {r}"));
        if let Some((_, p)) = revs.iter().find(|(n, _)| Some(n.value.as_str()) == rev) {
            apply(&mut entries, p, &label, &mut BTreeSet::new(), diags);
            if p.pcb.is_some() {
                pcb.clone_from(&p.pcb);
            }
            notes.extend(p.note.clone());
        }
        let applicable: Vec<&(Located<String>, Patch)> = reworks
            .iter()
            .filter(|(_, p)| match (&p.revisions, rev) {
                (None, _) => true,
                (Some(list), Some(r)) => list.value.iter().any(|x| x.value == r),
                (Some(_), None) => false,
            })
            .collect();
        out.push(Board {
            label: label.clone(),
            revision: rev.map(str::to_owned),
            reworks: Vec::new(),
            notes: notes.clone(),
            pcb: pcb.clone(),
            entries: entries.clone(),
        });
        for (name, p) in &applicable {
            let mut e = entries.clone();
            let l = format!("{label} + rework {}", name.value);
            apply(
                &mut e,
                p,
                &format!("rework {}", name.value),
                &mut BTreeSet::new(),
                diags,
            );
            out.push(Board {
                label: l,
                revision: rev.map(str::to_owned),
                reworks: vec![name.value.clone()],
                notes: notes.iter().chain(&p.note).cloned().collect(),
                pcb: pcb.clone(),
                entries: e,
            });
        }
        if applicable.len() > 1 {
            let mut e = entries.clone();
            let mut owner: BTreeMap<String, &str> = BTreeMap::new();
            for (name, p) in &applicable {
                let mut touched = BTreeSet::new();
                apply(
                    &mut e,
                    p,
                    &format!("rework {}", name.value),
                    &mut touched,
                    diags,
                );
                for t in touched {
                    if let Some(first) = owner.insert(t.clone(), &name.value)
                        && overlaps_reported.insert((
                            first.to_owned(),
                            name.value.clone(),
                            t.clone(),
                        ))
                    {
                        diags.push(
                            Diagnostic::error("rework-overlap", format!("reworks {first} and {} both change {t}: fitted together, which one holds?", name.value))
                                .at(&name.span)
                                .help("describe the combined change in one rework, or split the entry"),
                        );
                    }
                }
            }
            out.push(Board {
                label: format!(
                    "{label} + reworks {}",
                    applicable
                        .iter()
                        .map(|(n, _)| n.value.as_str())
                        .collect::<Vec<_>>()
                        .join(", ")
                ),
                revision: rev.map(str::to_owned),
                reworks: applicable.iter().map(|(n, _)| n.value.clone()).collect(),
                notes: notes
                    .iter()
                    .chain(applicable.iter().filter_map(|(_, p)| p.note.as_ref()))
                    .cloned()
                    .collect(),
                pcb,
                entries: e,
            });
        }
    }
    out
}
