//! The pin table (Markdown): every MCU pin with its header positions, what else sits on its net, its role and
//! the description's note.

use std::collections::BTreeMap;
use std::fmt::Write;

use anyhow::Result;

use crate::desc::{Desc, Role};
use crate::geometry::Geometry;
use crate::nets::Nets;
use crate::style::{NetClass, classify};
use crate::views::header_positions;

pub fn render(desc: &Desc, geo: &Geometry) -> Result<String> {
    let pin_re = regex_lite::Regex::new(&desc.board.pin_nets)?;
    let Some(mcu) = geo.footprints.get(&desc.board.mcu_ref) else {
        anyhow::bail!(
            "MCU footprint {} is not in the geometry",
            desc.board.mcu_ref
        );
    };
    let nets = Nets::new(geo, &pin_re);
    let headers = header_positions(desc, geo, &nets);
    let header_refs: Vec<&str> = desc
        .view
        .connector
        .iter()
        .map(|c| c.r#ref.as_str())
        .collect();

    // every footprint's pads by net, for the "also on the net" column
    let mut on_net: BTreeMap<&str, Vec<String>> = BTreeMap::new();
    for (reference, fp) in &geo.footprints {
        if reference == &desc.board.mcu_ref || header_refs.contains(&reference.as_str()) {
            continue;
        }
        for pad in &fp.pads {
            if let Some(net) = &pad.net {
                on_net
                    .entry(nets.canonical(net))
                    .or_default()
                    .push(reference.clone());
            }
        }
    }

    let mut rows: Vec<(String, String, String, String, String, String)> = Vec::new();
    let mut seen = std::collections::BTreeSet::new();
    // a pad nothing else is wired to (KiCad's `unconnected-...` net) gets no row, only the line under the table
    let mut unconnected: Vec<&str> = Vec::new();
    // a pin by its pad's pin name (KiCad's pinfunction, MAP-10), else by its group's name: `PB14` for a pad on the net
    // labelled `LED`; two MCU pins on one net (PB3 and PG9 shorted on the board) get a row each (MAP-11)
    let mut pads: Vec<(&str, &str)> = Vec::new();
    for p in &mcu.pads {
        let Some(label) = p.net.as_deref() else {
            continue;
        };
        let own = p
            .pin
            .as_deref()
            .and_then(|n| crate::nets::pin_name(n, &pin_re));
        if crate::nets::is_unconnected(label) {
            unconnected.extend(own);
            continue;
        }
        pads.push((own.unwrap_or_else(|| nets.canonical(label)), label));
    }
    pads.sort_by_key(|(name, _)| pin_key(name));
    // the MCU pins of each group, for "also on the net"
    let mut mcu_pins: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
    for &(name, label) in &pads {
        if pin_re.is_match(name) {
            let pins = mcu_pins.entry(nets.canonical(label)).or_default();
            if !pins.contains(&name) {
                pins.push(name);
            }
        }
    }
    for (net, label) in pads {
        if !pin_re.is_match(net) || !seen.insert(net.to_string()) {
            continue;
        }
        let group = nets.canonical(label);
        let class = classify(desc, &pin_re, Some(net));
        let role = match class {
            NetClass::Pin(Role::Free) => "free",
            NetClass::Pin(Role::Shared) => "shared",
            NetClass::Pin(Role::Taken) => "taken",
            NetClass::Pin(Role::Function) => "function",
            _ => "",
        };
        let alias = desc.aliases.get(net).cloned().unwrap_or_default();
        let on_headers = headers
            .get(group)
            .map(|h| h.join(", "))
            .unwrap_or_else(|| "—".to_string());
        let mut others: Vec<String> = mcu_pins
            .get(group)
            .into_iter()
            .flatten()
            .filter(|p| **p != net)
            .map(|p| format!("{p} ({})", desc.board.mcu_ref))
            .collect();
        let mut parts = on_net.get(group).cloned().unwrap_or_default();
        parts.sort();
        parts.dedup();
        others.extend(parts);
        let pin = desc.pins.get(net);
        let note = pin.map(|p| p.note.clone()).unwrap_or_default();
        let shown = crate::nets::short_name(label);
        rows.push((
            if shown == net || crate::nets::is_anonymous(label) {
                format!("`{net}`")
            } else {
                format!("`{net}` {shown}")
            },
            alias,
            on_headers,
            others.join(", "),
            role.to_string(),
            note,
        ));
    }
    unconnected.sort_by_key(|n| pin_key(n));
    unconnected.dedup();

    let mut out = String::new();
    let _ = writeln!(out, "# {} · pins\n", desc.board.name);
    let _ = writeln!(
        out,
        "Every MCU pin of the {} ({}): where it is on the headers, what else sits on its net, and whether an application may use it. Roles: **free** reaches the headers and nothing else; **shared** has a board job by default that a solder bridge or jumper removes; **taken** is the debug port, reset or wired to a part of the board for good; **function** is a crystal, VBAT or VREF pin.\n",
        desc.board.mcu, desc.board.pcb
    );
    // the alias column only where the description names any (a Nucleo's Arduino names)
    let aliases = !desc.aliases.is_empty();
    let _ = writeln!(
        out,
        "| Pin |{} Headers | Also on the net | Role | What it does on the board |",
        if aliases { " Arduino |" } else { "" }
    );
    let _ = writeln!(
        out,
        "|---|{}---|---|---|---|",
        if aliases { "---|" } else { "" }
    );
    for (pin, alias, headers, others, role, note) in &rows {
        let role_cell = match role.as_str() {
            "free" => "🟢 free",
            "shared" => "🟠 shared",
            "taken" => "🔴 taken",
            "function" => "🟣 function",
            _ => "",
        };
        let alias = if aliases {
            format!(" {alias} |")
        } else {
            String::new()
        };
        let _ = writeln!(
            out,
            "| {pin} |{alias} {headers} | {others} | {role_cell} | {note} |"
        );
    }
    if !unconnected.is_empty() {
        let _ = writeln!(
            out,
            "\nNot connected on the board ({}): {}.",
            unconnected.len(),
            unconnected.join(", ")
        );
    }
    if !desc.sb.is_empty() {
        let _ = writeln!(out, "\n## Solder bridges\n");
        let _ = writeln!(out, "| Bridge | Default | Side | Meaning |");
        let _ = writeln!(out, "|---|---|---|---|");
        for (reference, note) in &desc.sb {
            let fp = geo.footprints.get(reference);
            let default = fp
                .map(|f| {
                    if f.value.to_ascii_uppercase().contains("ON") {
                        "ON"
                    } else {
                        "OFF"
                    }
                })
                .unwrap_or("?");
            let side = fp
                .map(|f| match f.side {
                    crate::geometry::Side::F => "top",
                    crate::geometry::Side::B => "bottom",
                })
                .unwrap_or("?");
            let _ = writeln!(out, "| {reference} | {default} | {side} | {note} |");
        }
    }
    if !desc.board.sources.is_empty() {
        let _ = writeln!(
            out,
            "\nSources: {}. Generated by boardmap (bedrock_board) from `{}`.",
            desc.board.sources.join("; "),
            geo.source
        );
    }
    Ok(out)
}

/// Sort key: port letter, then pin number as a number (`PA2` before `PA10`).
fn pin_key(net: &str) -> (String, u32) {
    let digits: String = net.chars().filter(|c| c.is_ascii_digit()).collect();
    let letters: String = net.chars().filter(|c| !c.is_ascii_digit()).collect();
    (letters, digits.parse().unwrap_or(0))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geometry::{Footprint, Pad, Shape, Side};

    fn pad(number: &str, net: &str, pin: Option<&str>) -> Pad {
        Pad {
            number: number.into(),
            at: [0.0, 0.0],
            size: [1.0, 1.0],
            shape: Shape::Rect,
            through: false,
            net: Some(net.into()),
            pin: pin.map(Into::into),
        }
    }

    fn fp(pads: Vec<Pad>) -> Footprint {
        Footprint {
            value: String::new(),
            side: Side::F,
            at: [0.0, 0.0],
            rot: 0.0,
            pads,
        }
    }

    /// MAP-11: a BGA's unconnected balls, two MCU pins shorted on one net and pin names carrying a second function.
    #[test]
    fn shorted_pins_get_a_row_each_and_unconnected_balls_none() {
        let mut footprints = BTreeMap::new();
        footprints.insert(
            "U1".to_string(),
            fp(vec![
                pad("A6", "/MCU/PB3_PG9", Some("PB3")),
                pad("A8", "/MCU/PB3_PG9", Some("PG9")),
                pad("J1", "H7_CLK", Some("PH0-OSC_IN")),
                pad("P1", "unconnected-(U1A-PA0-PadP1)", Some("PA0")),
                pad("P2", "unconnected-(U1A-PA1-PadP2)", Some("PA1")),
            ]),
        );
        footprints.insert("J1".to_string(), fp(vec![pad("3", "/MCU/PB3_PG9", None)]));
        let geo = Geometry {
            source: "b.kicad_pcb".into(),
            bbox: [0.0; 4],
            outline: vec![],
            footprints,
        };
        let desc: Desc = toml::from_str(
            "[board]\nname = \"b\"\npcb = \"p\"\nmcu = \"m\"\nmcu_ref = \"U1\"\ngeometry = \"g.json\"\n\n\
             [[view.connector]]\nref = \"J1\"\nlabel = \"J1\"\nlabels = \"outside\"\n\n\
             [pins]\nPG9 = { role = \"taken\", note = \"CAN TX\" }\nPH0 = { role = \"function\", note = \"HSE\" }\n",
        )
        .unwrap();
        let table = render(&desc, &geo).unwrap();
        let row = |pin: &str| {
            table
                .lines()
                .find(|l| l.starts_with(&format!("| `{pin}`")))
                .unwrap_or_else(|| panic!("no row for {pin} in\n{table}"))
                .to_string()
        };
        assert_eq!(
            row("PB3"),
            "| `PB3` PB3_PG9 | J1-3 | PG9 (U1) | 🟢 free |  |"
        );
        assert_eq!(
            row("PG9"),
            "| `PG9` PB3_PG9 | J1-3 | PB3 (U1) | 🔴 taken | CAN TX |"
        );
        assert_eq!(row("PH0"), "| `PH0` H7_CLK | — |  | 🟣 function | HSE |");
        assert!(!table.contains("| `PA0`"), "{table}");
        assert!(
            table.contains("Not connected on the board (2): PA0, PA1."),
            "{table}"
        );
    }

    /// MAP-14: B200A's GP26 and GP34 are both on /MCU/SCK; each gets its row and names the other
    #[test]
    fn two_rp_pins_on_one_net_get_a_row_each() {
        let mut footprints = BTreeMap::new();
        footprints.insert(
            "U1".to_string(),
            fp(vec![
                pad("1", "/MCU/SCK", Some("GP26")),
                pad("2", "/MCU/SCK", Some("GP34")),
                pad("3", "LED", Some("GP38")),
            ]),
        );
        let geo = Geometry {
            source: String::new(),
            bbox: [0.0; 4],
            outline: vec![],
            footprints,
        };
        let desc: Desc = toml::from_str(concat!(
            "[board]\nname = \"b\"\npcb = \"p\"\nmcu = \"m\"\nmcu_ref = \"U1\"\ngeometry = \"g.json\"\n",
            "pin_nets = '^GP\\d+$'\n",
            "[pins]\nGP26 = { role = \"shared\" }\nGP34 = { role = \"free\" }\n",
        ))
        .unwrap();
        let md = render(&desc, &geo).unwrap();
        let rows: Vec<&str> = md.lines().filter(|l| l.starts_with("| `GP")).collect();
        assert_eq!(rows.len(), 3, "{md}");
        assert!(
            rows[0].starts_with("| `GP26` SCK |")
                && rows[0].contains("GP34 (U1)")
                && rows[0].contains("shared"),
            "{md}"
        );
        assert!(
            rows[1].starts_with("| `GP34` SCK |")
                && rows[1].contains("GP26 (U1)")
                && rows[1].contains("free"),
            "{md}"
        );
        assert!(rows[2].starts_with("| `GP38` LED |"), "{md}");
    }
}
