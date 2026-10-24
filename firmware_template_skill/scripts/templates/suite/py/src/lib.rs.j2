//! `nucleo_g0b1re` Python module: [Device] wraps `nucleo_g0b1re_client::Nucleo` with blocking calls on a tokio
//! runtime of its own, so scripts and notebooks need no asyncio. Everything the module does, the library does.

use std::path::PathBuf;
use std::time::Duration;

use nucleo_g0b1re_client::{
    BootTarget, CounterBuffer, Error, Nucleo, PROBE_RS_CHIP, Probe, SiliconRevision, api_line,
    firmware_line, node_lines, reset_lines,
};
use pyo3::exceptions::{PyConnectionError, PyRuntimeError, PyValueError};
use pyo3::prelude::*;
use pyo3::types::PyDict;
use tokio::runtime::Runtime;

/// Talk to the Nucleo G0B1RE firmware through its debug probe.
#[pymodule]
fn nucleo_g0b1re(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<Device>()?;
    m.add("PROBE_RS_CHIP", PROBE_RS_CHIP)?;
    m.add("__version__", env!("CARGO_PKG_VERSION"))?;
    Ok(())
}

fn to_py(e: Error) -> PyErr {
    match e {
        Error::Connect(_) => PyConnectionError::new_err(e.to_string()),
        Error::Invalid(_) | Error::Label(_) => PyValueError::new_err(e.to_string()),
        Error::Link { .. }
        | Error::Unimplemented { .. }
        | Error::Device { .. }
        | Error::Elf(_)
        | Error::Decode(_) => PyRuntimeError::new_err(e.to_string()),
    }
}

/// A board reached through a debug probe (RTT). Connects when created; use it as a context manager or call
/// `disconnect()` to release the probe.
///
/// `Device(probe_serial=None, chip="STM32G0B1RETx", elf=None, speed_hz=None)`: `probe_serial` picks a Nucleo
/// ST-LINK when several are attached (a serial or a part of it, or VID:PID:SERIAL; no other probe is ever used
/// without it), `elf` (the firmware's ELF) makes attaching fast and decodes counters and the log (default for those:
/// the ELF of the board's build id).
#[pyclass]
struct Device {
    runtime: Runtime,
    board: Option<Nucleo>,
    elf: Option<PathBuf>,
}

impl Device {
    fn nucleo(&self) -> PyResult<&Nucleo> {
        self.board
            .as_ref()
            .ok_or_else(|| PyConnectionError::new_err("disconnected"))
    }

    fn block<T>(&self, f: impl Future<Output = nucleo_g0b1re_client::Result<T>>) -> PyResult<T> {
        self.runtime.block_on(f).map_err(to_py)
    }
}

#[pymethods]
impl Device {
    #[new]
    #[pyo3(signature = (probe_serial=None, chip=None, elf=None, speed_hz=None))]
    fn new(
        probe_serial: Option<String>,
        chip: Option<String>,
        elf: Option<PathBuf>,
        speed_hz: Option<u32>,
    ) -> PyResult<Self> {
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .enable_all()
            .build()?;
        let mut probe = Probe {
            chip: chip.unwrap_or_else(|| PROBE_RS_CHIP.to_string()),
            speed_hz,
            elf: elf.clone(),
            ..Probe::default()
        };
        if let Some(spec) = probe_serial {
            probe = probe.select(&spec).map_err(to_py)?;
        }
        let board = runtime.block_on(Nucleo::connect(&probe)).map_err(to_py)?;
        Ok(Self {
            runtime,
            board: Some(board),
            elf,
        })
    }

    /// API name and version the board reported.
    fn api(&self) -> PyResult<String> {
        Ok(api_line(self.nucleo()?.info()))
    }

    /// Switch the user LED (LD4) on.
    fn led_on(&self) -> PyResult<()> {
        self.block(self.nucleo()?.led_on())
    }

    /// Switch the user LED off.
    fn led_off(&self) -> PyResult<()> {
        self.block(self.nucleo()?.led_off())
    }

    /// Blink the LED `count` times, `period_ms` per on-off cycle.
    #[pyo3(signature = (count=3, period_ms=500))]
    fn blink(&self, count: u32, period_ms: u64) -> PyResult<()> {
        self.block(
            self.nucleo()?
                .blink(count, Duration::from_millis(period_ms)),
        )
    }

    /// Name, version, git commit, build time and profile of the running firmware, as text.
    fn firmware(&self) -> PyResult<String> {
        Ok(firmware_line(&self.block(self.nucleo()?.firmware())?))
    }

    /// GNU build id of the running firmware, hex.
    fn build_id(&self) -> PyResult<String> {
        Ok(hex(&self.block(self.nucleo()?.build_id())?))
    }

    /// The name stored on the board.
    #[getter]
    fn label(&self) -> PyResult<String> {
        self.block(self.nucleo()?.label())
    }

    /// Store a name on the board (at most 32 bytes, in flash when the call returns).
    #[setter]
    fn set_label(&self, label: &str) -> PyResult<()> {
        self.block(self.nucleo()?.set_label(label))
    }

    /// Why the board last reset, with the crash record after a fault or panic, as text.
    fn last_reset(&self) -> PyResult<String> {
        let board = self.nucleo()?;
        let reset = self.block(board.last_reset())?;
        let boots = self.block(board.boot_count())?;
        Ok(reset_lines(&reset, boots).join("\n"))
    }

    /// Boots since the boot counter was cleared.
    fn boot_count(&self) -> PyResult<u32> {
        self.block(self.nucleo()?.boot_count())
    }

    /// Forget the crash record and zero the boot counter.
    fn clear_last_reset(&self) -> PyResult<()> {
        self.block(self.nucleo()?.clear_last_reset())
    }

    /// Board name, revision and serial number: `{"name", "revision", "serial"}`.
    fn board<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyDict>> {
        let b = self.block(self.nucleo()?.board())?;
        let d = PyDict::new(py);
        d.set_item("name", b.name.as_str())?;
        d.set_item("revision", b.revision.as_str())?;
        d.set_item("serial", b.serial.as_str())?;
        Ok(d)
    }

    /// The board's unique ids, hex: the STM32 96-bit unique id.
    fn uid(&self) -> PyResult<Vec<String>> {
        let ids = self.block(self.nucleo()?.uid())?;
        Ok(ids.iter().map(|id| hex(id.value.as_slice())).collect())
    }

    /// The chip on the board: `{"dev_id", "rev_id", "revision", "flash_kib"}`, `revision` is "A" or "unknown".
    fn variant<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyDict>> {
        let v = self.block(self.nucleo()?.variant())?;
        let d = PyDict::new(py);
        d.set_item("dev_id", v.dev_id)?;
        d.set_item("rev_id", v.rev_id)?;
        d.set_item(
            "revision",
            match v.revision {
                SiliconRevision::A => "A",
                _ => "unknown",
            },
        )?;
        d.set_item("flash_kib", v.flash_kib)?;
        Ok(d)
    }

    /// Every cnt counter, decoded with the firmware's ELF: `{"BUF/group/name": value}`, BUF is RAM or BKP.
    #[pyo3(signature = (force=false))]
    fn counters<'py>(&self, py: Python<'py>, force: bool) -> PyResult<Bound<'py, PyDict>> {
        let report = self.block(self.nucleo()?.counters(self.elf.as_deref(), force))?;
        let d = PyDict::new(py);
        for c in &report.counters {
            d.set_item(format!("{}/{}", c.buffer, c.name), c.value)?;
        }
        Ok(d)
    }

    /// Zero the counters of a buffer: "ram" or "bkp".
    fn reset_counters(&self, buffer: &str) -> PyResult<()> {
        let buffer = match buffer {
            "ram" => CounterBuffer::Ram,
            "bkp" => CounterBuffer::Backup,
            _ => return Err(PyValueError::new_err("buffer is \"ram\" or \"bkp\"")),
        };
        self.block(self.nucleo()?.reset_counters(buffer))
    }

    /// The defmt log the board keeps (since boot, and from before a crash), decoded: one string per line.
    #[pyo3(signature = (force=false))]
    fn log(&self, force: bool) -> PyResult<Vec<String>> {
        Ok(self
            .block(self.nucleo()?.log(self.elf.as_deref(), force))?
            .lines)
    }

    /// The LED after boot, kept in flash: `True` on, `False` off.
    #[getter]
    fn led_at_boot(&self) -> PyResult<bool> {
        self.block(self.nucleo()?.led_at_boot())
    }

    #[setter]
    fn set_led_at_boot(&self, on: bool) -> PyResult<()> {
        self.block(self.nucleo()?.set_led_at_boot(on))
    }

    /// Every setting back to its default, the label too.
    fn reset_settings(&self) -> PyResult<()> {
        self.block(self.nucleo()?.reset_settings())
    }

    /// The boot state: "Confirmed" (the firmware is the only image).
    fn boot_state(&self) -> PyResult<String> {
        Ok(format!("{:?}", self.block(self.nucleo()?.boot_state())?))
    }

    /// Reset the board into the firmware, or with `updater=True` into the STM32 ROM bootloader (USB DFU on
    /// PA11/PA12). The connection is gone afterwards: the object is disconnected.
    #[pyo3(signature = (updater=false))]
    fn reboot(&mut self, updater: bool) -> PyResult<()> {
        let into = if updater {
            BootTarget::Updater
        } else {
            BootTarget::App
        };
        self.block(self.nucleo()?.reboot(into))?;
        self.disconnect()
    }

    /// The update node (product, profile, boot state, last result), as text.
    fn dfu_node(&self) -> PyResult<String> {
        Ok(node_lines(&self.block(self.nucleo()?.dfu_node())?).join("\n"))
    }

    /// Everything of the device contract in one text, for a bug report (counters and log decoded with the ELF).
    #[pyo3(signature = (force=false))]
    fn report(&self, force: bool) -> PyResult<String> {
        Ok(self
            .runtime
            .block_on(self.nucleo()?.report(self.elf.clone(), force)))
    }

    /// Release the probe. The object can't be used afterwards.
    fn disconnect(&mut self) -> PyResult<()> {
        if let Some(board) = self.board.take() {
            self.runtime.block_on(board.disconnect());
        }
        Ok(())
    }

    fn __enter__(slf: PyRef<'_, Self>) -> PyRef<'_, Self> {
        slf
    }

    #[pyo3(signature = (*_args))]
    fn __exit__(&mut self, _args: &Bound<'_, pyo3::types::PyTuple>) -> PyResult<()> {
        self.disconnect()
    }

    fn __repr__(&self) -> String {
        match &self.board {
            Some(b) => format!("Device({})", api_line(b.info())),
            None => "Device(disconnected)".to_string(),
        }
    }
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}
