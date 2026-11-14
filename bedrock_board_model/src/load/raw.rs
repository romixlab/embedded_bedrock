//! The file as serde reads it: every field with its span, unknown fields refused.

use std::collections::BTreeMap;
use std::fmt;

use serde::Deserialize;
use serde::de::{self, MapAccess, Visitor};
use toml::Spanned;

use crate::schema::{Level, Pull};

macro_rules! with_sections {
    ($(#[$m:meta])* struct $name:ident { $($(#[$fm:meta])* $field:ident: $ty:ty,)* }) => {
        $(#[$m])*
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        pub(super) struct $name {
            $($(#[$fm])* pub $field: $ty,)*
            #[serde(default)]
            pub gpio: BTreeMap<String, Gpio>,
            #[serde(default)]
            pub periph: BTreeMap<String, Periph>,
            #[serde(default)]
            pub led: BTreeMap<String, Led>,
            #[serde(default)]
            pub button: BTreeMap<String, Button>,
            #[serde(default)]
            pub reserved: BTreeMap<String, Reserved>,
            #[serde(default)]
            pub short: BTreeMap<String, Short>,
            #[serde(default)]
            pub free: BTreeMap<Spanned<String>, Spanned<String>>,
        }

        impl $name {
            /// Moves the entry sections out.
            pub fn take_entries(&mut self) -> Entries {
                Entries {
                    gpio: std::mem::take(&mut self.gpio),
                    periph: std::mem::take(&mut self.periph),
                    led: std::mem::take(&mut self.led),
                    button: std::mem::take(&mut self.button),
                    reserved: std::mem::take(&mut self.reserved),
                    short: std::mem::take(&mut self.short),
                    free: std::mem::take(&mut self.free),
                }
            }
        }
    };
}

with_sections! {
    /// The whole file. Sections are written out in each table (not `#[serde(flatten)]`, which loses spans).
    struct File {
        board: Board,
        #[serde(default)]
        rev: BTreeMap<Spanned<String>, Patch>,
        #[serde(default)]
        rework: BTreeMap<Spanned<String>, Patch>,
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Board {
    pub name: String,
    pub chip: Spanned<String>,
    pub package: Option<Spanned<String>>,
    pub mcu: Option<Spanned<String>>,
    pub pcb: Option<Spanned<String>>,
    #[serde(default)]
    pub revisions: Vec<Spanned<String>>,
}

with_sections! {
    /// A `[rev.X]` or `[rework.N]` table: what changes against the base.
    struct Patch {
        note: Option<String>,
        pcb: Option<Spanned<String>>,
        revisions: Option<Spanned<Vec<Spanned<String>>>>,
        #[serde(default)]
        remove: Vec<Spanned<String>>,
    }
}

/// The entry sections of the base or of a patch.
pub(super) struct Entries {
    pub gpio: BTreeMap<String, Gpio>,
    pub periph: BTreeMap<String, Periph>,
    pub led: BTreeMap<String, Led>,
    pub button: BTreeMap<String, Button>,
    pub reserved: BTreeMap<String, Reserved>,
    pub short: BTreeMap<String, Short>,
    pub free: BTreeMap<Spanned<String>, Spanned<String>>,
}

#[derive(Deserialize, Clone, Copy)]
#[serde(rename_all = "lowercase")]
pub(super) enum Mode {
    Output,
    Input,
    Analog,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Gpio {
    pub pin: PinRef,
    pub mode: Spanned<Mode>,
    pub init: Option<Spanned<Level>>,
    pub pull: Option<Spanned<Pull>>,
    pub note: Option<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Periph {
    pub instance: Spanned<String>,
    pub pins: BTreeMap<String, PinRef>,
    pub note: Option<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Led {
    pub pin: Option<PinRef>,
    pub rgb: Option<Spanned<Vec<PinRef>>>,
    pub active: Level,
    pub note: Option<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Button {
    pub pin: PinRef,
    pub active: Level,
    pub pull: Pull,
    pub note: Option<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Reserved {
    pub pins: Vec<PinRef>,
    pub note: Option<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Short {
    pub pins: Spanned<Vec<Spanned<String>>>,
    pub note: Option<String>,
}

/// `"PB9"` or `{ pin = "PB9", net = "FDCAN1_TX" }`.
pub(super) type PinRef = Spanned<PinRefValue>;

pub(super) enum PinRefValue {
    Pin(String),
    Table {
        pin: Spanned<String>,
        net: Option<Spanned<String>>,
    },
}

impl<'de> Deserialize<'de> for PinRefValue {
    fn deserialize<D: de::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        struct V;
        impl<'de> Visitor<'de> for V {
            type Value = PinRefValue;
            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("a pin (\"PB9\") or { pin = \"PB9\", net = \"FDCAN1_TX\" }")
            }
            fn visit_str<E: de::Error>(self, v: &str) -> Result<PinRefValue, E> {
                Ok(PinRefValue::Pin(v.to_owned()))
            }
            fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<PinRefValue, A::Error> {
                let (mut pin, mut net) = (None, None);
                while let Some(key) = map.next_key::<String>()? {
                    match key.as_str() {
                        "pin" => pin = Some(map.next_value()?),
                        "net" => net = Some(map.next_value()?),
                        other => return Err(de::Error::unknown_field(other, &["pin", "net"])),
                    }
                }
                let pin = pin.ok_or_else(|| de::Error::missing_field("pin"))?;
                Ok(PinRefValue::Table { pin, net })
            }
        }
        d.deserialize_any(V)
    }
}
