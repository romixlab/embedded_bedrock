//! The description against the PCB's nets.

use std::collections::{BTreeMap, BTreeSet};

use crate::pcb::{McuPins, net_matches};
use crate::schema::Board;
use crate::source::Diagnostic;

pub(super) fn check(board: &Board, mcu: &McuPins, out: &mut Vec<Diagnostic>) {
    let e = &board.entries;
    let uses = e.uses();
    let mut named: BTreeSet<&str> = BTreeSet::new();
    for u in &uses {
        let pin = &u.pin.pin;
        named.insert(pin.as_str());
        match mcu.net(pin.as_str()) {
            None => out.push(
                Diagnostic::error(
                    "pcb-pad",
                    format!(
                        "{pin} ({}) has no pad on {}",
                        u.owner,
                        mcu.reference,
                        pin = pin.value
                    ),
                )
                .at(&pin.span),
            ),
            Some(None) => out.push(
                Diagnostic::error(
                    "pcb-unconnected",
                    format!("{} ({}) is not connected on the PCB", pin.value, u.owner),
                )
                .at(&pin.span),
            ),
            Some(Some(net)) => {
                if let Some(want) = &u.pin.net
                    && !net_matches(net, &want.value)
                {
                    out.push(
                        Diagnostic::error(
                            "pcb-net",
                            format!(
                                "{} ({}) is on net {net}, not {}",
                                pin.value, u.owner, want.value
                            ),
                        )
                        .at(&want.span),
                    );
                }
            }
        }
    }
    for (pin, at) in &e.free {
        named.insert(pin.as_str());
        match mcu.net(pin) {
            None => out.push(
                Diagnostic::error(
                    "pcb-pad",
                    format!("free pin {pin} has no pad on {}", mcu.reference),
                )
                .at(&at.span),
            ),
            Some(None) => out.push(
                Diagnostic::warning(
                    "pcb-free-unconnected",
                    format!("free pin {pin} is not connected on the PCB"),
                )
                .at(&at.span)
                .help("a pin that reaches nothing is not worth listing: drop it from [free]"),
            ),
            Some(Some(_)) => {}
        }
    }
    let mut groups: Vec<BTreeSet<&str>> = Vec::new();
    for (name, s) in &e.short {
        let nets: Vec<(&str, Option<Option<&str>>)> = s
            .pins
            .iter()
            .map(|p| (p.as_str(), mcu.net(p.as_str())))
            .collect();
        for p in &s.pins {
            named.insert(p.as_str());
        }
        if let [(first, first_net), rest @ ..] = nets.as_slice() {
            for (pin, net) in rest {
                if net != first_net || net.is_none_or(|n| n.is_none()) {
                    let show = |n: &Option<Option<&str>>| match n {
                        None => "no pad".to_owned(),
                        Some(None) => "no net".to_owned(),
                        Some(Some(n)) => format!("net {n}"),
                    };
                    out.push(
                        Diagnostic::error(
                            "pcb-short",
                            format!(
                                "short.{name}: {first} is on {}, {pin} on {}",
                                show(first_net),
                                show(net)
                            ),
                        )
                        .at(&s.pins[0].span),
                    );
                    break;
                }
            }
        }
        groups.push(s.pins.iter().map(|p| p.as_str()).collect());
    }
    let mut by_net: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
    for (pin, net) in &mcu.nets {
        if let Some(net) = net {
            by_net.entry(net).or_default().push(pin);
            if !named.contains(pin.as_str()) {
                out.push(
                    Diagnostic::warning(
                        "pcb-undescribed",
                        format!("{pin} is on net {net}, but the description does not name it"),
                    )
                    .help(format!(
                        "describe its job, or add it to [free]: {pin} = \"what it reaches\""
                    )),
                );
            }
        }
    }
    for (net, pins) in by_net {
        if pins.len() > 1 && !groups.iter().any(|g| pins.iter().all(|p| g.contains(p))) {
            out.push(
                Diagnostic::error(
                    "pcb-shared",
                    format!(
                        "{} are on one net, {net}, but no [short] says so",
                        pins.join(" and ")
                    ),
                )
                .help(format!("[short.NAME] pins = {pins:?}")),
            );
        }
    }
}
