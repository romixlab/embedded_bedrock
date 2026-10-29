//! The board description (TOML): what to draw and what to say about every pin, next to the geometry JSON.

use std::collections::BTreeMap;
use std::path::Path;

use anyhow::{Context, Result};
use serde::Deserialize;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Desc {
    pub board: Board,
    #[serde(default)]
    pub view: View,
    /// Alias shown next to a net's name on headers: `PA5 = "D13 / SCK"`.
    #[serde(default)]
    pub aliases: BTreeMap<String, String>,
    /// What every MCU pin does on the board, by its net name.
    #[serde(default)]
    pub pins: BTreeMap<String, Pin>,
    /// Solder bridges: a few words each, by reference.
    #[serde(default)]
    pub sb: BTreeMap<String, String>,
    #[serde(default)]
    pub power: Power,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Board {
    pub name: String,
    /// PCB name and revision as printed on the board.
    pub pcb: String,
    pub mcu: String,
    /// Reference of the MCU footprint.
    pub mcu_ref: String,
    /// Geometry JSON, relative to this file.
    pub geometry: String,
    /// Regex for the nets that are MCU pins (`^P[A-F]\d+$` by default).
    #[serde(default = "default_pin_pattern")]
    pub pin_nets: String,
    /// Documents the facts come from, for the footer.
    #[serde(default)]
    pub sources: Vec<String>,
    /// Pixels per millimetre of the board views.
    #[serde(default = "default_scale")]
    pub scale: f64,
}

fn default_pin_pattern() -> String {
    r"^P[A-K]\d+$".to_string()
}

fn default_scale() -> f64 {
    9.0
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct View {
    /// Connectors drawn pad by pad, with a label per pad.
    #[serde(default)]
    pub connector: Vec<Connector>,
    /// Other parts drawn as a labelled box.
    #[serde(default)]
    pub part: Vec<Part>,
    /// A dashed line across the board, mm from the top: a break-off line, for example.
    #[serde(default)]
    pub cut: Vec<f64>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Connector {
    pub r#ref: String,
    pub label: String,
    /// Where the pad labels go: left, right (outside the board), inside (next to the pads), or outside: turned on their
    /// side above the top row and below the bottom row, for a header lying across the board (MAP-11).
    pub labels: Labels,
    /// Show pin numbers next to the pads.
    #[serde(default)]
    pub numbers: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Labels {
    Left,
    Right,
    InsideLeft,
    InsideRight,
    Outside,
    None,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Part {
    pub r#ref: String,
    pub label: String,
    #[serde(default)]
    pub kind: PartKind,
    /// Label offset from the part's centre, px.
    #[serde(default)]
    pub dx: f64,
    #[serde(default)]
    pub dy: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PartKind {
    #[default]
    Other,
    Mcu,
    Button,
    Led,
    Usb,
    Jumper,
    Chip,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Pin {
    pub role: Role,
    /// A few characters shown on the header label, `SWDIO`.
    #[serde(default)]
    pub short: String,
    /// The full story, for the pin table.
    #[serde(default)]
    pub note: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Role {
    /// Reaches a header and nothing else on the board.
    Free,
    /// Has a job on the board by default (an LED, a button, the ST-LINK UART) that a solder bridge or jumper removes.
    Shared,
    /// Not usable for the application (debug port, reset).
    Taken,
    /// A fixed function pin: crystal, VBAT, VREF.
    Function,
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Power {
    #[serde(default)]
    pub node: Vec<PowerNode>,
    #[serde(default)]
    pub edge: Vec<PowerEdge>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PowerNode {
    pub id: String,
    /// Two lines at most, separated by `\n`.
    pub label: String,
    pub kind: PowerKind,
    /// Grid position.
    pub col: f64,
    pub row: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PowerKind {
    Source,
    Switch,
    Regulator,
    Rail,
    Load,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PowerEdge {
    pub from: String,
    pub to: String,
    #[serde(default)]
    pub label: String,
    /// Drawn dashed: an option that is off by default.
    #[serde(default)]
    pub off: bool,
}

impl Desc {
    pub fn load(path: &Path) -> Result<Self> {
        let text =
            std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
        toml::from_str(&text).with_context(|| format!("parsing {}", path.display()))
    }
}
