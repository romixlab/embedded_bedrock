//! Chip data for the checks: GPIO names, the pins of each package, and which pins carry each peripheral signal
//! with which alternate function. Read from stm32-data's chip JSON (embassy-rs/stm32-data-generated
//! `data/chips/<CHIP>.json`), the source of stm32-metapac's metadata. stm32-metapac itself holds one chip per
//! build (a cargo feature), so a host tool that checks any board reads the JSON.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

use serde::Deserialize;

/// What the checks need to know about a chip.
#[derive(Debug, Clone, Default)]
pub struct ChipData {
    /// `STM32H725IG`.
    pub name: String,
    /// Every pin of the die that firmware can use (`PA0`, and analog-only pads like `PA0_C`).
    pub gpios: BTreeSet<String>,
    pub packages: Vec<Package>,
    /// Peripheral instance (`FDCAN1`) to the pins of its signals.
    pub peripherals: BTreeMap<String, Vec<SignalPin>>,
}

/// A package of the chip and the GPIOs it brings out.
#[derive(Debug, Clone)]
pub struct Package {
    /// Part name (`STM32H725IGKx`).
    pub name: String,
    /// Package name (`UFBGA176`).
    pub package: String,
    pub gpios: BTreeSet<String>,
}

/// A pin that can carry a peripheral's signal.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SignalPin {
    pub signal: String,
    pub pin: String,
    /// Alternate function number; `None` for an analog or dedicated function (ADC input, oscillator).
    pub af: Option<u8>,
}

/// Why chip data could not be read.
#[derive(Debug)]
pub enum ChipError {
    Json(serde_json::Error),
    /// The file has no core with pins: not a chip file of stm32-data.
    NoCore,
}

impl fmt::Display for ChipError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Json(_) => f.write_str("not stm32-data chip JSON"),
            Self::NoCore => f.write_str("stm32-data chip JSON without a core"),
        }
    }
}

impl std::error::Error for ChipError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Json(e) => Some(e),
            Self::NoCore => None,
        }
    }
}

#[derive(Deserialize)]
struct RawChip {
    name: String,
    #[serde(default)]
    packages: Vec<RawPackage>,
    cores: Vec<RawCore>,
}

#[derive(Deserialize)]
struct RawPackage {
    name: String,
    package: String,
    #[serde(default)]
    pins: Vec<RawPackagePin>,
}

#[derive(Deserialize)]
struct RawPackagePin {
    #[serde(default)]
    signals: Vec<String>,
}

#[derive(Deserialize)]
struct RawCore {
    #[serde(default)]
    peripherals: Vec<RawPeripheral>,
    #[serde(default)]
    pins: Vec<RawPin>,
}

#[derive(Deserialize)]
struct RawPin {
    name: String,
}

#[derive(Deserialize)]
struct RawPeripheral {
    name: String,
    #[serde(default)]
    pins: Vec<RawSignalPin>,
}

#[derive(Deserialize)]
struct RawSignalPin {
    pin: String,
    signal: String,
    af: Option<u8>,
}

impl ChipData {
    /// Reads stm32-data's chip JSON. Every core of a multi-core chip sees the same pins; the first is taken.
    pub fn from_stm32_data(json: &str) -> Result<Self, ChipError> {
        let raw: RawChip = serde_json::from_str(json).map_err(ChipError::Json)?;
        let core = raw.cores.into_iter().next().ok_or(ChipError::NoCore)?;
        let mut gpios: BTreeSet<String> = core.pins.into_iter().map(|p| p.name).collect();
        if gpios.is_empty() {
            return Err(ChipError::NoCore);
        }
        // the core's pin list leaves out the analog-only pads (H7's `PA0_C`); its peripherals name them
        gpios.extend(
            core.peripherals
                .iter()
                .flat_map(|p| p.pins.iter().map(|s| s.pin.clone())),
        );
        let packages = raw
            .packages
            .into_iter()
            .map(|p| Package {
                name: p.name,
                package: p.package,
                gpios: p
                    .pins
                    .into_iter()
                    .flat_map(|pin| pin.signals)
                    .filter(|s| gpios.contains(s))
                    .collect(),
            })
            .collect();
        let peripherals = core
            .peripherals
            .into_iter()
            .map(|p| {
                let pins = p
                    .pins
                    .into_iter()
                    .map(|s| SignalPin {
                        signal: s.signal,
                        pin: s.pin,
                        af: s.af,
                    })
                    .collect();
                (p.name, pins)
            })
            .collect();
        Ok(Self {
            name: raw.name,
            gpios,
            packages,
            peripherals,
        })
    }

    /// A package by its package name (`UFBGA176`) or part name (`STM32H725IGKx`).
    pub fn package(&self, name: &str) -> Option<&Package> {
        self.packages
            .iter()
            .find(|p| p.package.eq_ignore_ascii_case(name) || p.name.eq_ignore_ascii_case(name))
    }

    /// The pins that can carry `signal` of `instance`.
    pub fn signal_pins<'a>(
        &'a self,
        instance: &str,
        signal: &'a str,
    ) -> impl Iterator<Item = &'a SignalPin> {
        self.peripherals
            .get(instance)
            .into_iter()
            .flatten()
            .filter(move |s| s.signal == signal)
    }
}
