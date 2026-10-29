//! Nets as the board really has them: a fitted solder bridge (`SB_ON`, a 0 Ω part) joins two nets that the board
//! file keeps apart, so a header pin on `NetCN7_29` is really `PF0` when SB25 is on.

use std::collections::BTreeMap;

use crate::geometry::Geometry;

pub struct Nets {
    canonical: BTreeMap<String, String>,
}

/// A net name the board file made up (`NetCN7_29`, `unconnected-...`), not a label from the schematic.
pub fn is_anonymous(net: &str) -> bool {
    net.is_empty() || net.starts_with("Net") || net.to_ascii_lowercase().starts_with("unconnected")
}

impl Nets {
    /// Joins the nets across every fitted solder bridge (reference `SB*`, value containing `ON`) and every pad's pin
    /// name that is an MCU pin (`pin_re`) with the pad's net, and picks one name per group: an MCU pin name first,
    /// then a schematic label, then the shortest.
    pub fn new(geo: &Geometry, pin_re: &regex_lite::Regex) -> Self {
        let mut parent: BTreeMap<String, String> = BTreeMap::new();
        fn find(parent: &mut BTreeMap<String, String>, n: &str) -> String {
            let p = parent.get(n).cloned().unwrap_or_else(|| n.to_string());
            if p == n {
                return p;
            }
            let root = find(parent, &p);
            parent.insert(n.to_string(), root.clone());
            root
        }
        for (reference, fp) in &geo.footprints {
            if !reference.starts_with("SB") || !fp.value.to_ascii_uppercase().contains("ON") {
                continue;
            }
            let nets: Vec<&str> = fp.pads.iter().filter_map(|p| p.net.as_deref()).collect();
            for pair in nets.windows(2) {
                let (a, b) = (find(&mut parent, pair[0]), find(&mut parent, pair[1]));
                if a != b {
                    parent.insert(a, b);
                }
            }
        }
        // an MCU pad's pin name (KiCad's pinfunction) joins its net, so a net labelled `LED` is the pin `PB14`
        for fp in geo.footprints.values() {
            for pad in &fp.pads {
                if let (Some(net), Some(pin)) = (&pad.net, &pad.pin)
                    && pin_re.is_match(pin)
                {
                    let (a, b) = (find(&mut parent, net), find(&mut parent, pin));
                    if a != b {
                        parent.insert(a, b);
                    }
                }
            }
        }
        let mut groups: BTreeMap<String, Vec<String>> = BTreeMap::new();
        let all: Vec<String> = geo
            .footprints
            .values()
            .flat_map(|fp| fp.pads.iter())
            .flat_map(|p| {
                p.net
                    .iter()
                    .chain(p.pin.iter().filter(|n| pin_re.is_match(n)))
            })
            .cloned()
            .collect();
        for n in all {
            let root = find(&mut parent, &n);
            let g = groups.entry(root).or_default();
            if !g.contains(&n) {
                g.push(n);
            }
        }
        let mut canonical = BTreeMap::new();
        for members in groups.values() {
            let best = members
                .iter()
                .min_by_key(|n| (!pin_re.is_match(n), is_anonymous(n), n.len(), n.to_string()))
                .cloned()
                .unwrap_or_default();
            for m in members {
                canonical.insert(m.clone(), best.clone());
            }
        }
        Self { canonical }
    }

    /// The group's name for `net`.
    pub fn canonical<'a>(&'a self, net: &'a str) -> &'a str {
        self.canonical.get(net).map(String::as_str).unwrap_or(net)
    }

    /// What to print for a pad on `net`: its own name, or the group's when its own is made up; a hierarchical sheet's
    /// net without its sheet path (`/MCU/IO7_EXT` → `IO7_EXT`).
    pub fn display<'a>(&'a self, net: &'a str) -> &'a str {
        let net = if is_anonymous(net) {
            self.canonical(net)
        } else {
            net
        };
        match net.strip_prefix('/') {
            Some(path) => path.rsplit('/').next().unwrap_or(net),
            None => net,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geometry::{Footprint, Pad, Shape, Side};
    use std::collections::BTreeMap;

    fn fp(value: &str, nets: &[&str]) -> Footprint {
        Footprint {
            value: value.into(),
            side: Side::B,
            at: [0.0, 0.0],
            rot: 0.0,
            pads: nets
                .iter()
                .enumerate()
                .map(|(i, n)| Pad {
                    number: (i + 1).to_string(),
                    at: [0.0, 0.0],
                    size: [1.0, 1.0],
                    shape: Shape::Rect,
                    through: false,
                    net: Some(n.to_string()),
                    pin: None,
                })
                .collect(),
        }
    }

    #[test]
    fn fitted_bridges_join_nets_and_open_ones_do_not() {
        let mut footprints = BTreeMap::new();
        footprints.insert("SB25".to_string(), fp("SB_ON", &["PF0", "NetCN7_29"]));
        footprints.insert("SB23".to_string(), fp("SB_OFF", &["PC14", "NetCN7_25"]));
        footprints.insert("SB31".to_string(), fp("SB_ON", &["GND", "AGND"]));
        let geo = Geometry {
            source: String::new(),
            bbox: [0.0; 4],
            outline: vec![],
            footprints,
        };
        let re = regex_lite::Regex::new(r"^P[A-F]\d+$").unwrap();
        let nets = Nets::new(&geo, &re);
        assert_eq!(nets.canonical("NetCN7_29"), "PF0");
        assert_eq!(nets.display("NetCN7_29"), "PF0");
        assert_eq!(nets.canonical("NetCN7_25"), "NetCN7_25");
        assert_eq!(nets.canonical("AGND"), "GND");
        assert_eq!(
            nets.display("AGND"),
            "AGND",
            "a pad keeps its own schematic label"
        );
        assert_eq!(nets.display("/MCU/IO7_EXT"), "IO7_EXT", "no sheet path");
    }

    #[test]
    fn a_pads_pin_name_names_its_labelled_net() {
        let mut mcu = fp("STM32", &["LED", "PA0", "GND"]);
        mcu.pads[0].pin = Some("PB14".into());
        mcu.pads[1].pin = Some("PA0".into());
        mcu.pads[2].pin = Some("VSS".into());
        let mut footprints = BTreeMap::new();
        footprints.insert("U1".to_string(), mcu);
        footprints.insert("D14".to_string(), fp("LED", &["LED", "GND"]));
        let geo = Geometry {
            source: String::new(),
            bbox: [0.0; 4],
            outline: vec![],
            footprints,
        };
        let re = regex_lite::Regex::new(r"^P[A-K]\d+$").unwrap();
        let nets = Nets::new(&geo, &re);
        assert_eq!(nets.canonical("LED"), "PB14");
        assert_eq!(
            nets.display("LED"),
            "LED",
            "a pad keeps its own schematic label"
        );
        assert_eq!(nets.canonical("PA0"), "PA0");
        assert_eq!(nets.canonical("GND"), "GND", "VSS is no MCU pin");
    }
}
