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
    // a pin by its group's name: `PB14` for a pad on the net labelled `LED` (the pad's pin name, MAP-10)
    let mut pads: Vec<_> = mcu
        .pads
        .iter()
        .filter_map(|p| p.net.as_deref().map(|n| (nets.canonical(n), n)))
        .collect();
    pads.sort_by_key(|(name, _)| pin_key(name));
    for (net, label) in pads {
        if !pin_re.is_match(net) || !seen.insert(net.to_string()) {
            continue;
        }
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
            .get(net)
            .map(|h| h.join(", "))
            .unwrap_or_else(|| "—".to_string());
        let mut others = on_net.get(net).cloned().unwrap_or_default();
        others.sort();
        others.dedup();
        let pin = desc.pins.get(net);
        let note = pin.map(|p| p.note.clone()).unwrap_or_default();
        rows.push((
            if label == net || crate::nets::is_anonymous(label) {
                format!("`{net}`")
            } else {
                format!("`{net}` {label}")
            },
            alias,
            on_headers,
            others.join(", "),
            role.to_string(),
            note,
        ));
    }

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
