//! Host library for the Nucleo G0B1RE firmware: typed access to its WireWeaver API (`nucleo_g0b1re_api`) through
//! the debug probe (RTT). The command line (`nucleo_g0b1re_cli`) and the Python module (`nucleo_g0b1re_py`) are
//! thin layers over this crate: everything that talks to the board lives here.
//!
//! ```no_run
//! # async fn f() -> nucleo_g0b1re_client::Result<()> {
//! use nucleo_g0b1re_client::{Nucleo, Probe, firmware_line};
//! let board = Nucleo::connect(&Probe::default()).await?; // the only Nucleo ST-LINK attached
//! board.led_on().await?;
//! println!("{}", firmware_line(&board.firmware().await?));
//! board.disconnect().await;
//! # Ok(()) }
//! ```
//!
//! Adding to the API: declare the item in `nucleo_g0b1re_api/src/lib.rs`, implement it in `firmware/src/ww.rs`,
//! then add a method to [Nucleo] that calls the generated client ([Nucleo::raw]: `raw.led_on().call()`,
//! `raw.device().label().read_name()`, ...) through [call], [call_fallible], [read] or [write], so the CLI and
//! Python get it with the same errors. The device contract (`device`) is in `device.rs`.

mod device;

pub use device::*;

use std::path::PathBuf;
use std::time::Duration;

use wire_weaver::prelude::DeserializeShrinkWrapOwned;
use wire_weaver::shrink_wrap::MaxLen;
use wire_weaver_client::{
    CallError, Commander, DynClient, PreparedCall, PreparedRead, PreparedWrite,
};

pub use wire_weaver_client::{ClientConfig, DeviceApiInfo, WwClient};
pub use ww_firmware_info::{FirmwareSummaryOwned, ResetRecordOwned};
pub use ww_user_info::UserInfoErrorOwned;

/// probe-rs name of the MCU on the Nucleo-G0B1RE board (`probe-rs chip list`).
pub const PROBE_RS_CHIP: &str = "STM32G0B1RETx";

/// Longest label the firmware keeps (`UserInfo::CAPABILITIES`).
pub const MAX_LABEL_LEN: usize = ww_user_info::MAX_NAME_LEN;

/// Errors of the typed methods.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// Connecting failed: no probe, the chip did not answer, the RTT control block was not found, the API on the
    /// board is not `nucleo_g0b1re_api`.
    #[error("connecting failed: {0:#}")]
    Connect(#[source] anyhow::Error),
    /// The call did not go through: the link dropped, a timeout, an encoding problem.
    #[error("{item}: {source}")]
    Link {
        /// API item, e.g. `FirmwareInfo::FIRMWARE`.
        item: &'static str,
        #[source]
        source: wire_weaver_client::Error,
    },
    /// The firmware has no implementation of the item (an older firmware).
    #[error("{item} is not implemented by the firmware")]
    Unimplemented { item: &'static str },
    /// The firmware refused to store the label (too long, flash busy or faulted).
    #[error("label: {0:?}")]
    Label(UserInfoErrorOwned),
    /// The firmware refused a call with its error (counters, settings, boot).
    #[error("{item}: {message}")]
    Device { item: &'static str, message: String },
    /// The firmware's ELF, needed to decode counters and the log, was not found or does not match.
    #[error("firmware ELF: {0}")]
    Elf(String),
    /// Counters or the log could not be decoded.
    #[error("{0}")]
    Decode(String),
    /// A value refused before it was sent, with the reason.
    #[error("{0}")]
    Invalid(String),
}

pub type Result<T, E = Error> = std::result::Result<T, E>;

/// USB vendor and product id of the Nucleo-64's on-board ST-LINK/V2-1: the only probes [Probe::default] looks at, so
/// a probe of another board on the same PC is never used.
pub const NUCLEO_PROBE: (u16, u16) = (0x0483, 0x374b);

/// How to reach the board: which probe and chip, and how to find the RTT control block.
#[derive(Clone, Debug)]
pub struct Probe {
    /// probe-rs chip name, [PROBE_RS_CHIP] by default.
    pub chip: String,
    /// USB vendor and product id of the probe, [NUCLEO_PROBE] by default; `None`: any probe.
    pub vid_pid: Option<(u16, u16)>,
    /// Part of the probe's serial number, when several probes are attached.
    pub serial: Option<String>,
    /// SWD clock in Hz, the probe's default when `None`.
    pub speed_hz: Option<u32>,
    /// The firmware's ELF: its `_SEGGER_RTT` symbol gives the RTT control block address, so connecting takes tens
    /// of milliseconds instead of a scan of the RAM (about 1.5 s through an ST-LINK).
    pub elf: Option<PathBuf>,
}

impl Default for Probe {
    fn default() -> Self {
        Self {
            chip: PROBE_RS_CHIP.to_string(),
            vid_pid: Some(NUCLEO_PROBE),
            serial: None,
            speed_hz: None,
            elf: None,
        }
    }
}

impl Probe {
    /// Takes a probe as probe-rs names it (`PROBE_RS_PROBE`, `--probe`): `VID:PID` or `VID:PID:SERIAL` in hex, or a
    /// serial number (or a part of it) alone, which keeps the vendor and product id as they are.
    pub fn select(mut self, spec: &str) -> Result<Self> {
        let parts: Vec<&str> = spec.split(':').collect();
        let id = |s: &str| {
            u16::from_str_radix(s, 16)
                .map_err(|_| Error::Invalid(format!("not a probe VID:PID[:SERIAL]: {spec:?}")))
        };
        match parts.as_slice() {
            [serial] => self.serial = Some(serial.to_string()),
            [vid, pid] => self.vid_pid = Some((id(vid)?, id(pid)?)),
            [vid, pid, serial] => {
                self.vid_pid = Some((id(vid)?, id(pid)?));
                self.serial = Some(serial.to_string());
            }
            _ => {
                return Err(Error::Invalid(format!(
                    "not a probe VID:PID[:SERIAL]: {spec:?}"
                )));
            }
        }
        Ok(self)
    }
}

/// The generated client of `NucleoG0b1reApi`: one method per item of the API tree, built by `ww_codegen!` from
/// `nucleo_g0b1re_api/src/lib.rs`. [Nucleo] sits on top of it.
pub struct NucleoClient {
    cmd: Commander,
}

mod generated {
    wire_weaver::ww_codegen!(
        nucleo_g0b1re_api :: NucleoG0b1reApi for super::NucleoClient,
        client = "std_client",
        // debug_to_file = "../target/nucleo_g0b1re_client.rs",
    );
}

impl WwClient for NucleoClient {
    fn default_config() -> ClientConfig {
        ClientConfig::new().rtt(PROBE_RS_CHIP.to_string(), None)
    }

    fn from_cmd(cmd: Commander) -> Self {
        Self { cmd }
    }
}

/// A connected board: [Nucleo::connect], then the typed methods; [Nucleo::raw] for the rest of the API.
pub struct Nucleo {
    raw: NucleoClient,
    /// Clients of the device contract's global traits on the same connection, for what `ww_report` decodes
    /// (counters, the stored log, the report)
    contract: ww_report::clients::Contract,
}

/// One line for a firmware summary: `nucleo_g0b1re 0.1.0 (a1b2c3d-dirty, built 2026-10-23 10:20 UTC, release)`.
pub fn firmware_line(fw: &FirmwareSummaryOwned) -> String {
    let mut facts = Vec::new();
    if let Some(git) = &fw.git {
        let hex: String = git.commit.iter().map(|b| format!("{b:02x}")).collect();
        let short: String = hex.chars().take(7).collect();
        facts.push(if git.dirty {
            format!("{short}-dirty")
        } else {
            short
        });
    }
    if let Some(built) = &fw.built {
        let secs = built.time.secs.value();
        facts.push(format!(
            "built {}-{:02}-{:02} {:02}:{:02} UTC",
            built.date.year.year(),
            built.date.month.value(),
            built.date.day.value(),
            secs / 3600,
            secs % 3600 / 60
        ));
    }
    facts.push(match &fw.profile {
        ww_firmware_info::BuildProfileOwned::Release => "release".to_string(),
        ww_firmware_info::BuildProfileOwned::Debug => "debug".to_string(),
        ww_firmware_info::BuildProfileOwned::Other(name) => name.as_str().to_string(),
        _ => "unknown profile".to_string(),
    });
    format!(
        "{} {:?} ({})",
        fw.name.as_str(),
        fw.version,
        facts.join(", ")
    )
}

/// One line for the board: `NUCLEO-G0B1RE rev C02 (Assumed), serial 21002200175036344B333720`.
pub fn board_line(b: &BoardSummaryOwned) -> String {
    let mut line = format!(
        "{} rev {} ({:?})",
        b.name.as_str(),
        b.revision.as_str(),
        b.revision_source
    );
    for (label, v) in [("variant", &b.variant), ("BOM", &b.bom_variant)] {
        if !v.as_str().is_empty() {
            line.push_str(&format!(", {label} {}", v.as_str()));
        }
    }
    if !b.serial.as_str().is_empty() {
        line.push_str(&format!(", serial {}", b.serial.as_str()));
    }
    line
}

/// The last reset as lines: `Software, boot 2 since cleared`, then the panic message and the fault registers when
/// the record has them (`g0b1re report` names the crash's addresses with the ELF).
pub fn reset_lines(r: &ResetRecordOwned, boots: u32) -> Vec<String> {
    let mut first = format!("{:?}, boot {boots} since cleared", r.cause);
    if r.boots_since_crash > 0 {
        first.push_str(&format!(", crash data {} boots old", r.boots_since_crash));
    }
    if let Some(up) = &r.uptime {
        first.push_str(&format!(", ran {:.1} s", up.to_f64()));
    }
    let mut lines = vec![first];
    if !r.message.as_str().is_empty() {
        lines.push(format!("message: {}", r.message.as_str()));
    }
    match &r.fault {
        Some(ww_firmware_info::Fault::CortexM(c)) => lines.push(format!(
            "Cortex-M fault: pc {:#010x} lr {:#010x} sp {:#010x} xpsr {:#010x} cfsr {:#010x} hfsr {:#010x}",
            c.pc, c.lr, c.sp, c.xpsr, c.cfsr, c.hfsr
        )),
        Some(other) => lines.push(format!("{other:?}")),
        None => {}
    }
    lines
}

/// The update node as `name  value` lines: product, role, profile, boot state, app version, last result.
pub fn node_lines(n: &NodeInfoOwned) -> Vec<String> {
    let version = n
        .app_version
        .map_or_else(|| "none".to_string(), |v| v.to_string());
    vec![
        format!(
            "product      {} (id {:#010x})",
            n.product.as_str(),
            n.product_id
        ),
        format!("role         {:?}", n.role),
        format!("profile      {:?}", n.profile),
        format!("boot state   {:?}", n.boot_state),
        format!("app version  {version} (floor {})", n.version_floor),
        format!(
            "last result  {:?} (version {})",
            n.last_result.outcome, n.last_result.version
        ),
    ]
}

/// The API a connected board serves, name and version: `nucleo_g0b1re_api 0.1.0`.
pub fn api_line(info: &DeviceApiInfo) -> String {
    let v = &info.user_api_version;
    format!("{} {:?}", v.crate_id, v.version)
}

/// A method call of the generated client, with [Error] instead of the client's error.
pub async fn call<T: DeserializeShrinkWrapOwned>(
    item: &'static str,
    p: PreparedCall<T>,
) -> Result<T> {
    p.call()
        .await
        .map_err(|source| Error::Link { item, source })
}

/// A method call of the generated client whose error type is `E` (`Result<T, E>` in the API), with [Error] instead of
/// the client's error; the firmware's own error becomes [Error::Device].
pub async fn call_fallible<T, E>(item: &'static str, p: PreparedCall<T, CallError<E>>) -> Result<T>
where
    T: DeserializeShrinkWrapOwned,
    E: DeserializeShrinkWrapOwned + std::fmt::Debug + Send + Sync + 'static,
{
    p.call().await.map_err(|e| match e {
        CallError::Device(e) => Error::Device {
            item,
            message: format!("{e:?}"),
        },
        CallError::Unimplemented { .. } => Error::Unimplemented { item },
        CallError::Client(source) => Error::Link { item, source },
    })
}

/// A property or const read of the generated client, with [Error] instead of the client's error.
pub async fn read<T: DeserializeShrinkWrapOwned + std::fmt::Debug>(
    item: &'static str,
    p: PreparedRead<T>,
) -> Result<T> {
    p.read()
        .await
        .map_err(|source| Error::Link { item, source })
}

/// A property write of the generated client whose error type is `E`, mapped through `device`.
pub async fn write<E>(
    item: &'static str,
    p: PreparedWrite<CallError<E>>,
    device: impl FnOnce(E) -> Error,
) -> Result<()>
where
    E: DeserializeShrinkWrapOwned + std::fmt::Debug + Send + Sync + 'static,
{
    p.write().await.map_err(|e| match e {
        CallError::Device(e) => device(e),
        CallError::Unimplemented { .. } => Error::Unimplemented { item },
        CallError::Client(source) => Error::Link { item, source },
    })
}

impl Nucleo {
    /// Connects through `probe` (the only Nucleo ST-LINK attached, with `Probe::default()`).
    pub async fn connect(probe: &Probe) -> Result<Self> {
        let raw = NucleoClient::config(|c| {
            let mut c = c.rtt(probe.chip.clone(), probe.speed_hz);
            if let Some(elf) = &probe.elf {
                c = c.rtt_elf(elf);
            }
            if let Some((vid, pid)) = probe.vid_pid {
                c = c.usb_vid_pid(vid, pid);
            }
            match &probe.serial {
                Some(serial) => c.serial_contains(serial.clone()),
                None => c,
            }
        })
        .connect()
        .await
        .map_err(Error::Connect)?;
        Ok(Self::from_raw(raw))
    }

    /// Wraps a client connected another way (`NucleoClient::new().connect()`, `NucleoClient::config(..)`).
    pub fn from_raw(raw: NucleoClient) -> Self {
        let contract = ww_report::clients::Contract::new(&DynClient::from_cmd(raw.cmd.clone()));
        Self { raw, contract }
    }

    /// The generated client, for every item of the API tree without a typed method here.
    pub fn raw(&self) -> &NucleoClient {
        &self.raw
    }

    /// The API, link and protocol versions the board reported when connecting.
    pub fn info(&self) -> &DeviceApiInfo {
        self.raw.cmd.info()
    }

    /// Switches the user LED (LD4, PA5) on.
    pub async fn led_on(&self) -> Result<()> {
        call("NucleoG0b1reApi::led_on", self.raw.led_on()).await
    }

    /// Switches the user LED off.
    pub async fn led_off(&self) -> Result<()> {
        call("NucleoG0b1reApi::led_off", self.raw.led_off()).await
    }

    /// Blinks the LED `count` times, `period` per on-off cycle; the LED is off afterwards.
    pub async fn blink(&self, count: u32, period: Duration) -> Result<()> {
        for _ in 0..count {
            self.led_on().await?;
            tokio::time::sleep(period / 2).await;
            self.led_off().await?;
            tokio::time::sleep(period / 2).await;
        }
        Ok(())
    }

    /// Name, version, git commit, build time and profile of the running firmware.
    pub async fn firmware(&self) -> Result<FirmwareSummaryOwned> {
        read(
            "FirmwareInfo::FIRMWARE",
            self.raw.device().firmware().read_FIRMWARE(),
        )
        .await
    }

    /// GNU build id of the running firmware (20 bytes with sha1): finds the ELF it was built from.
    pub async fn build_id(&self) -> Result<Vec<u8>> {
        let id = read(
            "FirmwareInfo::BUILD_ID",
            self.raw.device().firmware().read_BUILD_ID(),
        )
        .await?;
        Ok(id.into_inner())
    }

    /// The name the user gave this board, kept in the board's flash.
    pub async fn label(&self) -> Result<String> {
        let name = read("UserInfo::name", self.raw.device().label().read_name()).await?;
        Ok(name.into_inner())
    }

    /// Stores a name for this board, at most [MAX_LABEL_LEN] bytes; it is in flash when the call returns.
    pub async fn set_label(&self, label: &str) -> Result<()> {
        let value = MaxLen::new(label.to_string()).map_err(|_| {
            Error::Invalid(format!(
                "label is longer than {MAX_LABEL_LEN} bytes: {label:?}"
            ))
        })?;
        write(
            "UserInfo::name",
            self.raw.device().label().write_name(value),
            Error::Label,
        )
        .await
    }

    /// Why the board last reset and, after a crash, the record the fault or panic handler saved.
    pub async fn last_reset(&self) -> Result<ResetRecordOwned> {
        call(
            "LastReset::last_reset",
            self.raw.device().last_reset().last_reset(),
        )
        .await
    }

    /// Boots since the boot counter was cleared.
    pub async fn boot_count(&self) -> Result<u32> {
        call(
            "LastReset::boot_count",
            self.raw.device().last_reset().boot_count(),
        )
        .await
    }

    /// Forgets the crash record and zeroes the boot counter.
    pub async fn clear_last_reset(&self) -> Result<()> {
        call("LastReset::clear", self.raw.device().last_reset().clear()).await
    }

    /// Closes the connection and releases the probe.
    pub async fn disconnect(self) {
        let _ = self.raw.cmd.disconnect().asynch().await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ww_firmware_info::{BuildProfileOwned, VersionOwned};

    #[test]
    fn firmware_line_without_git_and_build_time() {
        let fw = FirmwareSummaryOwned {
            name: MaxLen::new("nucleo_g0b1re".to_string()).unwrap(),
            version: VersionOwned::new(0, 1, 0),
            git: None,
            built: None,
            profile: BuildProfileOwned::Release,
        };
        assert_eq!(firmware_line(&fw), "nucleo_g0b1re 0.1.0 (release)");
    }

    #[test]
    fn probe_select() {
        let p = Probe::default().select("066AFF51").unwrap();
        assert_eq!(
            (p.vid_pid, p.serial.as_deref()),
            (Some(NUCLEO_PROBE), Some("066AFF51"))
        );
        let p = Probe::default().select("1366:0105").unwrap();
        assert_eq!((p.vid_pid, p.serial), (Some((0x1366, 0x0105)), None));
        let p = Probe::default().select("0483:374b:066AFF515754").unwrap();
        assert_eq!(
            (p.vid_pid, p.serial.as_deref()),
            (Some(NUCLEO_PROBE), Some("066AFF515754"))
        );
        assert!(Probe::default().select("x:y").is_err());
    }

    #[test]
    fn api_line_of_an_empty_info() {
        assert_eq!(api_line(&DeviceApiInfo::empty()), " 0.0.0");
    }
}
