//! Pins used twice, free pins in use, shorted pins used together.

use std::collections::BTreeMap;

use crate::schema::{Board, PinUse};
use crate::source::Diagnostic;

pub(super) fn check(board: &Board, out: &mut Vec<Diagnostic>) {
    let e = &board.entries;
    let uses = e.uses();
    let mut by_pin: BTreeMap<&str, Vec<&PinUse<'_>>> = BTreeMap::new();
    for u in &uses {
        by_pin.entry(u.pin.pin.as_str()).or_default().push(u);
    }
    for (pin, us) in &by_pin {
        if let [first, rest @ ..] = us.as_slice() {
            for u in rest {
                out.push(
                    Diagnostic::error(
                        "pin-twice",
                        format!("{pin} is used by {} and by {}", first.owner, u.owner),
                    )
                    .at(&u.pin.pin.span),
                );
            }
        }
    }
    for (pin, at) in &e.free {
        if let Some(us) = by_pin.get(pin.as_str()) {
            out.push(
                Diagnostic::error(
                    "free-used",
                    format!("{pin} is listed free, but {} uses it", us[0].owner),
                )
                .at(&at.span)
                .help("drop it from [free]"),
            );
        }
    }
    let mut group_of: BTreeMap<&str, &str> = BTreeMap::new();
    for (name, s) in &e.short {
        let mut taken = Vec::new();
        for p in &s.pins {
            if let Some(other) = group_of.insert(p.as_str(), name) {
                out.push(
                    Diagnostic::error(
                        "short-twice",
                        format!("{p} is in short.{other} and short.{name}: make it one group"),
                    )
                    .at(&p.span),
                );
            }
            if let Some(us) = by_pin.get(p.as_str()) {
                taken.push((p, us[0].owner.clone()));
            } else if e.free.contains_key(p.as_str()) {
                taken.push((p, "[free]".to_owned()));
            }
        }
        if let [(a, a_owner), rest @ ..] = taken.as_slice() {
            for (b, b_owner) in rest {
                out.push(
                    Diagnostic::error(
                        "short-both",
                        format!("{a} and {b} are joined on the board (short.{name}), but both are used: {a_owner}, {b_owner}"),
                    )
                    .at(&b.span)
                    .help("use one of them; the other stays an input"),
                );
            }
        }
    }
}
