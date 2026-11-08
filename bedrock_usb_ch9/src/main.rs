//! `bedrock-usb-ch9`: nusb-based chapter 9 checks against a USB device's descriptors, wLength edges, halt and
//! toggle behaviour, SET_CONFIGURATION / SET_INTERFACE loops and bus resets (USBD-6). It does not depend on
//! `bedrock_usb` and talks to any USB device through its standard requests, so it runs against today's
//! embassy-usb Donguru, usb_io_fw on RP2350, or a `bedrock_usb` firmware alike.
//!
//! Linux and macOS only for now: the control transfers run on `nusb::Device` directly (no interface claimed),
//! which `nusb` does not support on Windows.

#[cfg(target_os = "windows")]
compile_error!(
    "bedrock-usb-ch9 uses nusb::Device::control_in/control_out, which nusb does not support on Windows; \
     Windows support needs claim_interface(0) instead (USBD-6 follow-up), not done yet"
);

mod ch9;
mod checks;
mod report;
mod select;

use std::process::ExitCode;
use std::time::Duration;

use anstream::{eprintln, println};
use anstyle::{AnsiColor, Color, Style};
use clap::{Args, Parser, Subcommand, ValueEnum};
use clap_complete::CompleteEnv;
use nusb::MaybeFuture;

use report::{Outcome, Report};
use select::Selector;

const VERSION: &str = concat!(
    env!("CARGO_PKG_VERSION"),
    " (",
    env!("GIT_SHA"),
    ", commit ",
    env!("BUILD_TIME"),
    ")"
);

const BOLD: Style = Style::new().bold();
const DIM: Style = Style::new().dimmed();
const GREEN: Style = Style::new().fg_color(Some(Color::Ansi(AnsiColor::Green)));
const RED: Style = Style::new().fg_color(Some(Color::Ansi(AnsiColor::Red)));

/// Chapter 9 checks against a USB device, over nusb.
#[derive(Parser)]
#[command(
    name = "bedrock-usb-ch9",
    version = VERSION,
    after_help = "Examples:
  bedrock-usb-ch9 list                                 every USB device on the bus
  bedrock-usb-ch9 info --vid c0de --pid cafe            its descriptors
  bedrock-usb-ch9 run --vid c0de --pid cafe             the full chapter 9 suite
  bedrock-usb-ch9 run --vid c0de --pid cafe --only halt,wlength

Completion: source <(COMPLETE=bash bedrock-usb-ch9)   (zsh, fish likewise)"
)]
struct Cli {
    #[command(subcommand)]
    command: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// List connected USB devices, optionally filtered by VID/PID
    List {
        #[arg(long, value_parser = parse_hex16)]
        vid: Option<u16>,
        #[arg(long, value_parser = parse_hex16)]
        pid: Option<u16>,
    },
    /// Print a device's device, configuration and BOS descriptors
    Info {
        #[command(flatten)]
        device: DeviceArgs,
        /// Per-transfer timeout
        #[arg(long, default_value_t = 1000)]
        timeout_ms: u64,
    },
    /// Run the chapter 9 checks
    #[command(
        after_help = "Exit status: 0 everything passed, 1 something failed, 2 the device could not be \
        opened or an argument was wrong."
    )]
    Run {
        #[command(flatten)]
        device: DeviceArgs,
        /// Only these checks (default: all of them)
        #[arg(long, value_delimiter = ',')]
        only: Vec<Check>,
        /// Per-transfer timeout
        #[arg(long, default_value_t = 1000)]
        timeout_ms: u64,
        /// SET_CONFIGURATION / SET_INTERFACE cycles
        #[arg(long, default_value_t = 20)]
        config_loops: u32,
        /// Bus resets
        #[arg(long, default_value_t = 20)]
        reset_loops: u32,
        /// How long to wait for the device to re-enumerate after a bus reset
        #[arg(long, default_value_t = 5000)]
        reenumerate_timeout_ms: u64,
    },
}

#[derive(Args)]
struct DeviceArgs {
    /// Vendor id, hex (with or without a 0x prefix)
    #[arg(long, value_parser = parse_hex16)]
    vid: u16,
    /// Product id, hex (with or without a 0x prefix)
    #[arg(long, value_parser = parse_hex16)]
    pid: u16,
    /// Serial number, when more than one device shares the VID:PID
    #[arg(long)]
    serial: Option<String>,
}

impl DeviceArgs {
    fn selector(&self) -> Selector {
        Selector {
            vid: self.vid,
            pid: self.pid,
            serial: self.serial.clone(),
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, ValueEnum)]
enum Check {
    Wlength,
    Halt,
    Config,
    Reset,
}

fn parse_hex16(s: &str) -> Result<u16, String> {
    let s = s
        .strip_prefix("0x")
        .or_else(|| s.strip_prefix("0X"))
        .unwrap_or(s);
    u16::from_str_radix(s, 16).map_err(|e| format!("{s:?}: {e}"))
}

fn main() -> ExitCode {
    CompleteEnv::with_factory(<Cli as clap::CommandFactory>::command).complete();
    match run(Cli::parse()) {
        Ok(code) => code,
        Err(e) => {
            eprintln!("{RED}error{RED:#}: {e:#}");
            ExitCode::from(2)
        }
    }
}

fn run(cli: Cli) -> anyhow::Result<ExitCode> {
    match cli.command {
        Cmd::List { vid, pid } => list(vid, pid),
        Cmd::Info { device, timeout_ms } => info(&device, Duration::from_millis(timeout_ms)),
        Cmd::Run {
            device,
            only,
            timeout_ms,
            config_loops,
            reset_loops,
            reenumerate_timeout_ms,
        } => run_checks(
            &device,
            &only,
            Duration::from_millis(timeout_ms),
            config_loops,
            reset_loops,
            Duration::from_millis(reenumerate_timeout_ms),
        ),
    }
}

fn list(vid: Option<u16>, pid: Option<u16>) -> anyhow::Result<ExitCode> {
    let devices = nusb::list_devices().wait()?;
    let mut n = 0;
    for d in devices {
        if vid.is_some_and(|v| v != d.vendor_id()) || pid.is_some_and(|p| p != d.product_id()) {
            continue;
        }
        n += 1;
        println!(
            "{BOLD}{:04x}:{:04x}{BOLD:#} bus {:>3} addr {:>3} {DIM}serial{DIM:#} {:<16} {}",
            d.vendor_id(),
            d.product_id(),
            d.busnum(),
            d.device_address(),
            d.serial_number().unwrap_or("-"),
            d.product_string().unwrap_or("")
        );
    }
    if n == 0 {
        println!("{DIM}no matching USB device{DIM:#}");
    }
    Ok(ExitCode::SUCCESS)
}

fn info(device: &DeviceArgs, timeout: Duration) -> anyhow::Result<ExitCode> {
    let info = select::find_one(&device.selector())?;
    let dev = info.open().wait()?;
    let d = dev.device_descriptor();
    println!(
        "{BOLD}device{BOLD:#} usb {:x}.{:02x} class {:02x}/{:02x}/{:02x} vid:pid {:04x}:{:04x} rev {:04x} ep0 {}B",
        d.usb_version() >> 8,
        d.usb_version() & 0xff,
        d.class(),
        d.subclass(),
        d.protocol(),
        d.vendor_id(),
        d.product_id(),
        d.device_version(),
        d.max_packet_size_0()
    );
    for cfg in dev.configurations() {
        println!(
            "{BOLD}configuration {}{BOLD:#} {} bytes, {} interface(s), attributes 0x{:02x}, {}mA",
            cfg.configuration_value(),
            cfg.as_bytes().len(),
            cfg.num_interfaces(),
            cfg.attributes(),
            cfg.max_power() as u16 * 2
        );
        for intf in cfg.interfaces() {
            for alt in intf.alt_settings() {
                println!(
                    "  interface {} alt {} class {:02x}/{:02x}/{:02x}, {} endpoint(s)",
                    alt.interface_number(),
                    alt.alternate_setting(),
                    alt.class(),
                    alt.subclass(),
                    alt.protocol(),
                    alt.num_endpoints()
                );
                for ep in alt.endpoints() {
                    println!(
                        "    ep 0x{:02x} {:?} {:?} max {}B interval {}",
                        ep.address(),
                        ep.direction(),
                        ep.transfer_type(),
                        ep.max_packet_size(),
                        ep.interval()
                    );
                }
            }
        }
    }
    if let Ok(bos) = dev.get_descriptor(ch9::DESC_BOS, 0, 0, timeout).wait() {
        println!("{DIM}BOS: {} bytes{DIM:#}", bos.len());
    }
    Ok(ExitCode::SUCCESS)
}

fn run_checks(
    device: &DeviceArgs,
    only: &[Check],
    timeout: Duration,
    config_loops: u32,
    reset_loops: u32,
    reenumerate_timeout: Duration,
) -> anyhow::Result<ExitCode> {
    let sel = device.selector();
    let want = |c: Check| only.is_empty() || only.contains(&c);
    let info = select::find_one(&sel)?;
    let mut dev = info.open().wait()?;

    let mut report = Report::default();
    if want(Check::Wlength) {
        checks::wlength(&dev, timeout, &mut report);
    }
    if want(Check::Halt) {
        checks::halt(&dev, timeout, &mut report);
    }
    if want(Check::Config) {
        checks::config(&dev, config_loops, &mut report);
    }
    if want(Check::Reset) {
        dev = checks::bus_reset(&sel, dev, reset_loops, reenumerate_timeout, &mut report);
    }
    drop(dev);

    print_report(&report);
    Ok(ExitCode::from(report.exit_code()))
}

fn print_report(report: &Report) {
    for e in &report.entries {
        let rule = e.rule.map(|r| format!(" ({r})")).unwrap_or_default();
        match &e.outcome {
            Outcome::Pass => println!("{GREEN}PASS{GREEN:#} {}{DIM}{rule}{DIM:#}", e.name),
            Outcome::Fail(why) => println!("{RED}FAIL{RED:#} {}{DIM}{rule}{DIM:#}: {why}", e.name),
            Outcome::Skip(why) => println!("{DIM}SKIP {}{rule}: {why}{DIM:#}", e.name),
        }
    }
    println!(
        "{BOLD}{} passed, {} failed, {} skipped{BOLD:#}",
        report.passed(),
        report.failed(),
        report.skipped()
    );
}
