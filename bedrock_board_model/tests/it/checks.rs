//! Each check on its fixture.

use std::collections::BTreeMap;

use bedrock_board_model::{McuPins, Severity, check};

use crate::{codes, find, load, test_chip};

#[test]
fn clean_description_has_no_findings() {
    let d = load("minimal.toml");
    let r = check(&d, Some(&test_chip()), &BTreeMap::new());
    assert!(r.diagnostics.is_empty(), "{:#?}", r.diagnostics);
    assert_eq!(r.worst(), None);
}

#[test]
fn pin_twice_free_used_short_both() {
    let d = load("pins.toml");
    let r = check(&d, None, &BTreeMap::new());
    assert_eq!(
        codes(&d, &r.diagnostics),
        [("pin-twice", 8), ("short-both", 21), ("free-used", 25)]
    );
    assert_eq!(
        r.diagnostics[0].message,
        "PA0 is used by gpio.reset and by periph.can RX"
    );
    assert_eq!(
        r.diagnostics[2].message,
        "PA1 is listed free, but periph.can TX uses it"
    );
    assert!(
        r.diagnostics[1].message.contains("button.user, [free]"),
        "{}",
        r.diagnostics[1].message
    );
}

#[test]
fn pins_and_signals_against_the_chip() {
    let d = load("chip.toml");
    let r = check(&d, Some(&test_chip()), &BTreeMap::new());
    let found = codes(&d, &r.diagnostics);
    assert_eq!(
        found,
        [
            ("pin-unknown", 9),     // PC0 is on the die, not on TSSOP8
            ("signal-unknown", 13), // FDCAN2 has no RX
            ("signal-pin", 13),     // FDCAN2 TX is not on PA1
            ("periph-unknown", 16), // no SPI1
            ("pin-unknown", 20),    // PZ9
        ]
    );
    let pin_pkg = &r.diagnostics[0];
    assert_eq!(
        pin_pkg.message,
        "PC0 (periph.can1 TX) is not on the TSSOP8 package"
    );
    let sig = find(&r, "signal-pin")[0];
    assert_eq!(sig.message, "FDCAN2 TX is not on PA1 (periph.can2 TX)");
    assert_eq!(sig.help.as_deref(), Some("pins that carry it: PB1 (AF9)"));
    // PB0 carries FDCAN1 RX (AF3): no finding for can1 RX
    assert!(r.diagnostics.iter().all(|d| !d.message.contains("can1 RX")));
}

#[test]
fn unknown_package_names_the_ones_there_are() {
    let text = std::fs::read_to_string(crate::fixture("minimal.toml"))
        .unwrap()
        .replace("TSSOP8", "QFN99");
    let d = bedrock_board_model::Description::parse("m.toml".into(), text).unwrap();
    let r = check(&d, Some(&test_chip()), &BTreeMap::new());
    let p = find(&r, "package-unknown");
    assert_eq!(p.len(), 1);
    assert_eq!(p[0].help.as_deref(), Some("packages: TSSOP8"));
}

fn t7_pcb() -> McuPins {
    let net = |n: &str| Some(n.to_owned());
    McuPins {
        reference: "U1".to_owned(),
        nets: BTreeMap::from([
            ("PA0".to_owned(), net("/MCU/CAN_RX")),
            ("PA1".to_owned(), net("/MCU/CAN_TX")),
            ("PB0".to_owned(), None),
            ("PB1".to_owned(), net("J1_3")),
            ("PC0".to_owned(), net("J1_4")),
            ("PA0_C".to_owned(), net("J1_1")),
            ("PC5".to_owned(), net("J1_2")),
            ("PC6".to_owned(), net("J1_2")),
        ]),
    }
}

#[test]
fn description_against_the_pcb() {
    let d = load("pcb.toml");
    let pcbs = BTreeMap::from([("t7.json".to_owned(), t7_pcb())]);
    let r = check(&d, None, &pcbs);
    assert_eq!(
        codes(&d, &r.diagnostics),
        [
            ("pcb-net", 10),         // TX is on CAN_TX, not CAN_RX
            ("pcb-unconnected", 13), // the LED's pin goes nowhere
            ("pcb-short", 17),       // PB1 and PC0 are on different nets
            ("pcb-shared", 0),       // PC5 and PC6 share J1_2
            ("pcb-undescribed", 0),
            ("pcb-undescribed", 0),
        ]
    );
    // CAN_RX matches /MCU/CAN_RX by its last segment: no finding for RX
    assert_eq!(find(&r, "pcb-net").len(), 1);
    assert_eq!(
        find(&r, "pcb-shared")[0].message,
        "PC5 and PC6 are on one net, J1_2, but no [short] says so"
    );
    assert!(
        find(&r, "pcb-undescribed")
            .iter()
            .all(|d| d.severity == Severity::Warning)
    );
}

#[test]
fn findings_merge_across_configurations() {
    // pins.toml with two revisions, rev B moving the button off the short: pin-twice in both, short-both in A only
    let text = std::fs::read_to_string(crate::fixture("pins.toml"))
        .unwrap()
        .replace(
            "chip = \"STM32TEST\"",
            "chip = \"STM32TEST\"\nrevisions = [\"A\", \"B\"]",
        )
        + "\n[rev.B.button.user]\npin = \"PC0\"\nactive = \"low\"\npull = \"up\"\n";
    let d = bedrock_board_model::Description::parse("p.toml".into(), text).unwrap();
    let r = check(&d, None, &BTreeMap::new());
    assert_eq!(r.configs, ["rev A", "rev B"]);
    assert_eq!(find(&r, "pin-twice")[0].configs, ["rev A", "rev B"]);
    assert_eq!(find(&r, "short-both")[0].configs, ["rev A"]);
}
