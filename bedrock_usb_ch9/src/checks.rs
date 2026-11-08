//! The chapter 9 checks themselves, each against an already-open `nusb::Device`. Every one appends to a
//! `Report` rather than returning on the first failure, so one run shows everything that's wrong, not just the
//! first thing.
//!
//! What's deliberately not here: USB 2.0 §9.4.6 SET_ADDRESS (R14 of design/bedrock-usb.md) happens during the
//! OS's own enumeration, before a `nusb::Device` exists; there is nothing a user-space tool can address it to.
//! It is exercised indirectly by the `reset` check succeeding or not. Likewise the data toggle itself (R12)
//! isn't a value USB exposes to the host to read back; `halt` checks the halt bit through GET_STATUS, and
//! `reset`/`config` check that many cycles keep completing cleanly, which is what a stuck or desynced toggle
//! would eventually break.

use std::thread::sleep;
use std::time::{Duration, Instant};

use nusb::MaybeFuture;
use nusb::descriptors::TransferType;
use nusb::transfer::{Buffer, Bulk, In, Out, Recipient, TransferError};

use crate::ch9;
use crate::report::Report;
use crate::select::{Selector, find_one};

/// GET_DESCRIPTOR(DEVICE) and GET_DESCRIPTOR(CONFIGURATION) at a spread of wLength values around each
/// descriptor's actual size (R13).
pub fn wlength(dev: &nusb::Device, timeout: Duration, report: &mut Report) {
    let ep0 = dev.device_descriptor().max_packet_size_0() as usize;
    probe_descriptor(dev, "device", 0x01, 0, 18, ep0, timeout, report);
    match dev.configurations().next() {
        Some(cfg) => {
            let len = cfg.as_bytes().len();
            let value = cfg.configuration_value();
            probe_descriptor(
                dev,
                &format!("configuration {value}"),
                0x02,
                0,
                len,
                ep0,
                timeout,
                report,
            );
        }
        None => report.skip(
            "wlength: configuration descriptor",
            Some("R13"),
            "no configuration descriptor",
        ),
    }
}

#[allow(clippy::too_many_arguments)]
fn probe_descriptor(
    dev: &nusb::Device,
    label: &str,
    desc_type: u8,
    desc_index: u8,
    actual: usize,
    ep0_max_packet: usize,
    timeout: Duration,
    report: &mut Report,
) {
    for wlength in ch9::wlength_candidates(actual, ep0_max_packet) {
        let name = format!("wlength: {label} descriptor, wLength={wlength}");
        match raw_get_descriptor(dev, desc_type, desc_index, wlength, timeout) {
            Ok(data) => {
                let want = actual.min(wlength as usize);
                if data.len() == want {
                    report.pass(name, Some("R13"));
                } else {
                    report.fail(
                        name,
                        Some("R13"),
                        format!(
                            "got {} bytes, expected {want} (actual length {actual})",
                            data.len()
                        ),
                    );
                }
            }
            Err(e) => report.fail(name, Some("R13"), e.to_string()),
        }
    }
}

fn raw_get_descriptor(
    dev: &nusb::Device,
    desc_type: u8,
    desc_index: u8,
    wlength: u16,
    timeout: Duration,
) -> Result<Vec<u8>, TransferError> {
    use nusb::transfer::{ControlIn, ControlType};
    dev.control_in(
        ControlIn {
            control_type: ControlType::Standard,
            recipient: Recipient::Device,
            request: 0x06, // GET_DESCRIPTOR
            value: ((desc_type as u16) << 8) | desc_index as u16,
            index: 0,
            length: wlength,
        },
        timeout,
    )
    .wait()
}

/// SET_FEATURE / CLEAR_FEATURE(ENDPOINT_HALT) and the resulting STALL on every bulk endpoint of the active
/// configuration (USB 2.0 §9.4.5, R12).
pub fn halt(dev: &nusb::Device, timeout: Duration, report: &mut Report) {
    let config = match dev.active_configuration() {
        Ok(c) => c,
        Err(e) => {
            report.skip("halt", Some("R12"), e.to_string());
            return;
        }
    };
    let mut any = false;
    for intf in config.interfaces() {
        let alt = intf.first_alt_setting();
        let iface_num = alt.interface_number();
        for ep in alt.endpoints() {
            if ep.transfer_type() != TransferType::Bulk {
                continue;
            }
            any = true;
            let addr = ep.address();
            let dir_in = ep.direction() == nusb::transfer::Direction::In;
            let name = format!(
                "halt: ep 0x{addr:02x} ({}) on interface {iface_num}",
                if dir_in { "IN" } else { "OUT" }
            );
            match halt_one(dev, iface_num, addr, dir_in, ep.max_packet_size(), timeout) {
                Ok(()) => report.pass(name, Some("R12")),
                Err(e) => report.fail(name, Some("R12"), e.to_string()),
            }
        }
    }
    if !any {
        report.skip(
            "halt",
            Some("R12"),
            "no bulk endpoint in the active configuration",
        );
    }
}

fn halt_one(
    dev: &nusb::Device,
    iface_num: u8,
    addr: u8,
    dir_in: bool,
    max_packet: usize,
    timeout: Duration,
) -> anyhow::Result<()> {
    let before = ch9::get_status(dev, Recipient::Endpoint, addr as u16, timeout)?;
    anyhow::ensure!(
        before.len() >= 2 && before[0] & 1 == 0,
        "endpoint was already halted before the test started"
    );

    ch9::set_feature(
        dev,
        Recipient::Endpoint,
        ch9::ENDPOINT_HALT,
        addr as u16,
        timeout,
    )?;
    let halted = ch9::get_status(dev, Recipient::Endpoint, addr as u16, timeout)?;
    anyhow::ensure!(
        halted.len() >= 2 && halted[0] & 1 == 1,
        "SET_FEATURE(ENDPOINT_HALT) had no effect (GET_STATUS still reads 0)"
    );

    let interface = dev.claim_interface(iface_num).wait()?;
    let stalled = if dir_in {
        let mut ep = interface.endpoint::<Bulk, In>(addr)?;
        matches!(
            ep.transfer_blocking(Buffer::new(max_packet.max(1)), timeout)
                .into_result(),
            Err(TransferError::Stall)
        )
    } else {
        let mut ep = interface.endpoint::<Bulk, Out>(addr)?;
        matches!(
            ep.transfer_blocking(vec![0u8; 1].into(), timeout)
                .into_result(),
            Err(TransferError::Stall)
        )
    };
    anyhow::ensure!(
        stalled,
        "a transfer on the halted endpoint did not return STALL (USB 2.0 §9.4.5)"
    );

    ch9::clear_feature(
        dev,
        Recipient::Endpoint,
        ch9::ENDPOINT_HALT,
        addr as u16,
        timeout,
    )?;
    let after = ch9::get_status(dev, Recipient::Endpoint, addr as u16, timeout)?;
    anyhow::ensure!(
        after.len() >= 2 && after[0] & 1 == 0,
        "CLEAR_FEATURE(ENDPOINT_HALT) had no effect (R12: the halt feature must clear)"
    );
    Ok(())
}

/// SET_CONFIGURATION and SET_INTERFACE loops, each ending on an undefined value that must STALL (USB 2.0
/// §9.4.7, §9.4.10).
pub fn config(dev: &nusb::Device, loops: u32, report: &mut Report) {
    let configs: Vec<u8> = dev
        .configurations()
        .map(|c| c.configuration_value())
        .collect();
    let Some(&valid) = configs.first() else {
        report.skip(
            "set-configuration",
            None,
            "device descriptor reports 0 configurations",
        );
        return;
    };
    let invalid = (1u8..=250).find(|v| !configs.contains(v));

    let mut ok = true;
    for i in 0..loops.max(1) {
        if let Err(e) = dev.set_configuration(valid).wait() {
            report.fail(
                format!("set-configuration({valid}), loop {i}"),
                None,
                e.to_string(),
            );
            ok = false;
            break;
        }
        match dev.active_configuration() {
            Ok(c) if c.configuration_value() == valid => {}
            Ok(c) => {
                report.fail(
                    format!("set-configuration({valid}), loop {i}"),
                    None,
                    format!(
                        "active configuration reads back {}",
                        c.configuration_value()
                    ),
                );
                ok = false;
                break;
            }
            Err(e) => {
                report.fail(
                    format!("set-configuration({valid}), loop {i}"),
                    None,
                    e.to_string(),
                );
                ok = false;
                break;
            }
        }
    }
    if ok {
        report.pass(format!("set-configuration({valid}) loop x{loops}"), None);
    }

    if let Some(invalid) = invalid {
        match dev.set_configuration(invalid).wait() {
            Err(_) => report.pass(
                format!("set-configuration({invalid}) undefined, rejected"),
                None,
            ),
            Ok(()) => report.fail(
                format!("set-configuration({invalid}) undefined"),
                None,
                "device accepted an undefined configuration value",
            ),
        }
    }
    // leave the device configured for whatever runs next
    let _ = dev.set_configuration(valid).wait();

    set_interface(dev, report);
}

fn set_interface(dev: &nusb::Device, report: &mut Report) {
    let config = match dev.active_configuration() {
        Ok(c) => c,
        Err(e) => {
            report.skip("set-interface", Some("R12"), e.to_string());
            return;
        }
    };
    for intf in config.interfaces() {
        let num = intf.interface_number();
        let alts: Vec<u8> = intf.alt_settings().map(|a| a.alternate_setting()).collect();
        let Some(&first) = alts.first() else { continue };
        let iface = match dev.claim_interface(num).wait() {
            Ok(i) => i,
            Err(e) => {
                report.skip(
                    format!("set-interface(interface {num})"),
                    Some("R12"),
                    e.to_string(),
                );
                continue;
            }
        };
        let mut ok = true;
        for &alt in &alts {
            if let Err(e) = iface.set_alt_setting(alt).wait() {
                report.fail(
                    format!("set-interface(interface {num}, alt {alt})"),
                    Some("R12"),
                    e.to_string(),
                );
                ok = false;
                break;
            }
            if iface.get_alt_setting() != alt {
                report.fail(
                    format!("set-interface(interface {num}, alt {alt})"),
                    Some("R12"),
                    format!("get_alt_setting reads back {}", iface.get_alt_setting()),
                );
                ok = false;
                break;
            }
        }
        if ok {
            report.pass(
                format!("set-interface(interface {num}) alt cycle x{}", alts.len()),
                Some("R12"),
            );
        }

        if let Some(bad) = (0u8..=250).find(|a| !alts.contains(a)) {
            match iface.set_alt_setting(bad).wait() {
                Err(_) => report.pass(
                    format!("set-interface(interface {num}, alt {bad}) undefined, rejected"),
                    None,
                ),
                Ok(()) => report.fail(
                    format!("set-interface(interface {num}, alt {bad}) undefined"),
                    None,
                    "device accepted an undefined alternate setting",
                ),
            }
        }
        let _ = iface.set_alt_setting(first).wait();
    }
}

/// `loops` bus resets (R8's 10 ms re-enable deadline is the firmware's business; this checks that the device
/// re-enumerates and comes back as the same device every time). Returns the device re-opened after the last
/// reset, since the one passed in is no longer usable after the first `reset()`.
pub fn bus_reset(
    sel: &Selector,
    mut dev: nusb::Device,
    loops: u32,
    reenum_timeout: Duration,
    report: &mut Report,
) -> nusb::Device {
    for i in 0..loops.max(1) {
        if let Err(e) = dev.reset().wait() {
            report.fail(format!("bus-reset, loop {i}"), Some("R8"), e.to_string());
            return dev;
        }
        match reopen(sel, reenum_timeout) {
            Ok(d) => dev = d,
            Err(e) => {
                report.fail(
                    format!("bus-reset, loop {i}: re-enumeration"),
                    Some("R8"),
                    e.to_string(),
                );
                return dev;
            }
        }
    }
    report.pass(format!("bus-reset loop x{loops}"), Some("R8"));
    dev
}

fn reopen(sel: &Selector, timeout: Duration) -> anyhow::Result<nusb::Device> {
    let deadline = Instant::now() + timeout;
    loop {
        if let Ok(info) = find_one(sel)
            && let Ok(d) = info.open().wait()
        {
            return Ok(d);
        }
        if Instant::now() >= deadline {
            anyhow::bail!("did not re-enumerate as {sel} within {timeout:?}");
        }
        sleep(Duration::from_millis(20));
    }
}
