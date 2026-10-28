//! Colours and classification shared by the pictures and the pin table.

use crate::desc::{Desc, Role};

pub const FREE: &str = "#2e7d32";
pub const SHARED: &str = "#ef8f00";
pub const TAKEN: &str = "#c62828";
pub const FUNCTION: &str = "#6a1b9a";
pub const GND: &str = "#263238";
pub const V3: &str = "#f57c00";
pub const V5: &str = "#d84315";
pub const VIN: &str = "#8d2600";
pub const ANALOG: &str = "#7b1fa2";
pub const NC: &str = "#9e9e9e";
pub const BOARD_FILL: &str = "#eef1ee";
pub const BOARD_STROKE: &str = "#5f6b62";
pub const INK: &str = "#1f2a24";
pub const MUTED: &str = "#6b7570";
pub const LEADER: &str = "#b8c0bb";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NetClass {
    Pin(Role),
    Gnd,
    V3,
    V5,
    Vin,
    Analog,
    Reset,
    Other,
    Unconnected,
}

impl NetClass {
    pub fn color(self) -> &'static str {
        match self {
            NetClass::Pin(Role::Free) => FREE,
            NetClass::Pin(Role::Shared) => SHARED,
            NetClass::Pin(Role::Taken) => TAKEN,
            NetClass::Pin(Role::Function) => FUNCTION,
            NetClass::Gnd => GND,
            NetClass::V3 => V3,
            NetClass::V5 => V5,
            NetClass::Vin => VIN,
            NetClass::Analog => ANALOG,
            NetClass::Reset => TAKEN,
            NetClass::Other => NC,
            NetClass::Unconnected => NC,
        }
    }
}

pub fn classify(desc: &Desc, pin_re: &regex_lite::Regex, net: Option<&str>) -> NetClass {
    let Some(net) = net else {
        return NetClass::Unconnected;
    };
    if let Some(pin) = desc.pins.get(net) {
        return NetClass::Pin(pin.role);
    }
    if pin_re.is_match(net) {
        return NetClass::Pin(Role::Free);
    }
    // KiCad's power symbols are `+5V0`, `+VIN_CAN`: the `+` says nothing of the rail
    let up = net.trim_start_matches('+').to_ascii_uppercase();
    if up == "GND" || up == "AGND" || up == "VSS" || up.ends_with("GND") {
        NetClass::Gnd
    } else if up.starts_with("AVDD") || up.starts_with("VREF") || up.starts_with("AIN") {
        NetClass::Analog
    } else if up.contains("3V3")
        || up.starts_with("VDD")
        || up.starts_with("VBAT")
        || up.contains("3.3")
    {
        NetClass::V3
    } else if up.starts_with("VIN") || up.starts_with("E5V") {
        NetClass::Vin
    } else if up.contains("5V") || up.contains("VBUS") {
        NetClass::V5
    } else if up.contains("RST") || up.contains("RESET") {
        NetClass::Reset
    } else if up.starts_with("NET") || up.starts_with("UNCONNECTED") {
        NetClass::Unconnected
    } else {
        NetClass::Other
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_plus_in_front_of_a_rail_keeps_its_class() {
        let desc: Desc = toml::from_str(
            "[board]\nname = \"b\"\npcb = \"p\"\nmcu = \"m\"\nmcu_ref = \"U1\"\ngeometry = \"g.json\"\n",
        )
        .unwrap();
        let re = regex_lite::Regex::new(r"^P[A-K]\d+$").unwrap();
        assert_eq!(classify(&desc, &re, Some("+VIN_CAN")), NetClass::Vin);
        assert_eq!(classify(&desc, &re, Some("+VDDIO2")), NetClass::V3);
        assert_eq!(classify(&desc, &re, Some("+5V0_DCDC")), NetClass::V5);
        assert_eq!(classify(&desc, &re, Some("+3V3_DCDC")), NetClass::V3);
    }
}
