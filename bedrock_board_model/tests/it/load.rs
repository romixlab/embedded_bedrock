//! Loading: the schema, spans, structural mistakes, revisions and reworks.

use bedrock_board_model::{Description, GpioMode, LedPins, Level, LoadError};

use crate::{fixture, load};

fn invalid(name: &str) -> Vec<(&'static str, usize)> {
    match Description::load(&fixture(name)) {
        Err(LoadError::Invalid { file, diagnostics }) => diagnostics
            .iter()
            .map(|d| {
                (
                    d.code,
                    d.span.as_ref().map_or(0, |s| file.line_col(s.start).0),
                )
            })
            .collect(),
        Ok(_) => panic!("{name} loaded"),
        Err(e) => panic!("{name}: {e}"),
    }
}

#[test]
fn minimal_description_resolves_to_one_configuration() {
    let d = load("minimal.toml");
    assert_eq!(d.board.chip.value, "STM32TEST");
    let configs = d.configurations();
    assert_eq!(configs.len(), 1);
    assert_eq!(configs[0].label, "T1");
    let e = &configs[0].entries;
    assert_eq!(e.gpio["reset"].mode, GpioMode::Output { init: Level::High });
    assert_eq!(e.gpio["vsense"].mode, GpioMode::Analog);
    let tx = &e.periph["can"].pins["TX"];
    assert_eq!(tx.pin.value, "PA1");
    assert_eq!(tx.net.as_ref().map(|n| n.value.as_str()), Some("CAN_TX"));
    // the span of a pin given as a string is the string, quotes included
    let rx = &e.periph["can"].pins["RX"].pin;
    assert_eq!(&d.source.text[rx.span.clone()], "\"PA0\"");
    assert!(matches!(e.led["status"].pins, LedPins::Mono(_)));
    assert_eq!(e.uses().len(), 5);
}

#[test]
fn revisions_and_reworks_are_patches() {
    match Description::load(&fixture("revisions.toml")) {
        Err(LoadError::Invalid { file, diagnostics }) => {
            // rework 2 applies to every revision, rework 3 to rev B: fitted together on rev B, both set led.status
            let found: Vec<_> = diagnostics
                .iter()
                .map(|d| (d.code, file.line_col(d.span.clone().unwrap().start).0))
                .collect();
            assert_eq!(found, vec![("rework-overlap", 46)]);
        }
        Ok(_) => panic!("the overlap of reworks 2 and 3 on rev B was not found"),
        Err(e) => panic!("{e}"),
    }
}

#[test]
fn configurations_of_revisions_and_reworks() {
    // revisions.toml without rework 3: rev A has reworks 1 and 2, rev B rework 2
    let text = std::fs::read_to_string(fixture("revisions.toml")).unwrap();
    let cut = text.find("[rework.3]").unwrap();
    let d = Description::parse(fixture("revisions.toml"), text[..cut].to_owned()).unwrap();
    let labels: Vec<&str> = d
        .configurations()
        .iter()
        .map(|b| b.label.as_str())
        .collect();
    assert_eq!(
        labels,
        [
            "rev A",
            "rev A + rework 1",
            "rev A + rework 2",
            "rev A + reworks 1, 2",
            "rev B",
            "rev B + rework 2"
        ]
    );
    let by = |l: &str| d.configurations().iter().find(|b| b.label == l).unwrap();
    // rev A: own PCB, its LED, no reset line
    let a = by("rev A");
    assert_eq!(a.pcb.as_ref().unwrap().value, "a.json");
    assert_eq!(a.notes, ["first build"]);
    assert!(!a.entries.gpio.contains_key("reset"));
    assert!(matches!(&a.entries.led["status"].pins, LedPins::Mono(p) if p.pin.value == "PA0"));
    assert_eq!(a.entries.led["status"].active, Level::High);
    // rework 1 brings the reset line back on rev A
    assert_eq!(
        by("rev A + rework 1").entries.gpio["reset"].mode,
        GpioMode::Output { init: Level::High }
    );
    // both reworks: the line and the LED moved to PA1, PA1 no longer free
    let both = by("rev A + reworks 1, 2");
    assert!(both.entries.gpio.contains_key("reset"));
    assert!(both.entries.free.is_empty());
    assert_eq!(both.reworks, ["1", "2"]);
    assert_eq!(both.notes, ["first build", "reset line added by a wire"]);
    // rev B keeps the base PCB and the base entries
    let b = by("rev B");
    assert_eq!(b.pcb.as_ref().unwrap().value, "b.json");
    assert_eq!(
        b.entries.gpio["reset"].mode,
        GpioMode::Output { init: Level::Low }
    );
}

#[test]
fn structural_mistakes_point_at_their_lines() {
    let mut found = invalid("invalid.toml");
    found.sort();
    let mut want = vec![
        ("pcb-mcu", 6),          // a PCB but no mcu
        ("rev-twice", 5),        // "A" twice
        ("gpio-fields", 10),     // output without init: at `mode`
        ("gpio-fields", 15),     // init on an input
        ("led-rgb", 19),         // two pins for rgb
        ("short-pins", 23),      // one pin
        ("rev-unknown", 25),     // [rev.C]
        ("rework-revision", 29), // revision Z
        ("rework-pcb", 30),      // a rework with a PCB
    ];
    want.sort();
    assert_eq!(found, want);
}

#[test]
fn unknown_field_is_a_parse_error_with_its_line() {
    assert_eq!(invalid("unknown-field.toml"), vec![("parse", 9)]);
}

#[test]
fn removing_what_is_not_there() {
    let text = "[board]\nname = \"T\"\nchip = \"STM32TEST\"\nrevisions = [\"A\"]\n\n[rev.A]\nremove = [\"led.nothing\", \"bogus\"]\n";
    match Description::parse("t.toml".into(), text.to_owned()) {
        Err(LoadError::Invalid { diagnostics, .. }) => {
            let codes: Vec<_> = diagnostics.iter().map(|d| d.code).collect();
            assert_eq!(codes, ["remove-unknown", "remove-name"]);
        }
        other => panic!("{other:?}"),
    }
}
