#![cfg_attr(not(test), no_std)]
//! Crash capture for bedrock firmware (ROB-11, PLAT-7): why the device last reset, and what it saved before a crash.
//!
//! A `HardFault` or panic handler writes a [Record] (cause, uptime, Cortex-M registers, up to 32 stack words,
//! message, build id of the running image) into a small `.uninit` RAM area with a magic and a CRC, then resets. At
//! the next boot the firmware reads its reset-cause register ([stm32], [rp]) and calls `boot` (feature `cortex-m`),
//! which checks the record, increments the boot counter and returns a [Snapshot] for the `LastReset` server of
//! `ww_firmware_info`.
//!
//! The record survives resets, not power loss. After a power-on or brown-out the RAM is not read at all (on parts
//! with ECC RAM, such as the STM32H7, reading RAM that was never written can raise ECC errors), the record starts
//! empty and the boot counter at 1.
//!
//! A crash stays in the record until [Snapshot::clear] (or a power cycle), so a host connecting after more resets
//! still sees it; [Snapshot::cause] is always the cause of the last reset, so a crash older than the last reset
//! shows with a cause other than [Cause::Fault] / [Cause::Panic].
//!
//! This crate has no WireWeaver dependency: the firmware converts a [View] into `ww_firmware_info::ResetRecord`
//! (the bedrock template generates that code in `src/ww.rs`).

#[cfg(feature = "cortex-m")]
mod cortex_m;
pub mod rp;
pub mod stm32;

#[cfg(feature = "cortex-m")]
pub use cortex_m::*;

/// Cause of a reset, in the order and meaning of `ww_firmware_info::ResetCause`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
#[repr(u8)]
pub enum Cause {
    /// Power was applied.
    PowerOn = 0,
    /// Supply voltage dropped below the brown-out threshold.
    Brownout = 1,
    /// The reset pin was pulled low.
    Pin = 2,
    /// The watchdog was not fed in time.
    Watchdog = 3,
    /// Firmware asked for a reset (`sys_reset`).
    Software = 4,
    /// The core locked up (a fault inside a fault handler).
    Lockup = 5,
    /// A hardware fault, see [Crash::regs].
    Fault = 6,
    /// A Rust panic, see [Crash::message].
    Panic = 7,
    /// The reset that finished a firmware update.
    Updated = 8,
    /// The device cannot tell.
    Undetermined = 9,
}

impl Cause {
    fn from_u32(v: u32) -> Option<Self> {
        use Cause::*;
        Some(match v {
            0 => PowerOn,
            1 => Brownout,
            2 => Pin,
            3 => Watchdog,
            4 => Software,
            5 => Lockup,
            6 => Fault,
            7 => Panic,
            8 => Updated,
            9 => Undetermined,
            _ => return None,
        })
    }

    /// Power was lost: RAM content is undefined and must not be read.
    pub fn power_lost(self) -> bool {
        matches!(self, Cause::PowerOn | Cause::Brownout)
    }
}

/// Exception frame and fault status registers of a Cortex-M core, the fields of `ww_firmware_info::CortexMFault`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub struct CortexMRegs {
    /// Program counter at the fault.
    pub pc: u32,
    /// Link register.
    pub lr: u32,
    /// Program status register.
    pub xpsr: u32,
    /// Stack pointer before the exception entry.
    pub sp: u32,
    /// Configurable fault status (0 on ARMv6-M / ARMv8-M Baseline).
    pub cfsr: u32,
    /// Hard fault status (0 on ARMv6-M / ARMv8-M Baseline).
    pub hfsr: u32,
    /// MemManage fault address (0 on ARMv6-M / ARMv8-M Baseline).
    pub mmfar: u32,
    /// Bus fault address (0 on ARMv6-M / ARMv8-M Baseline).
    pub bfar: u32,
    /// `EXC_RETURN` of the exception entry, 0 when unknown.
    pub exc_return: u32,
}

impl CortexMRegs {
    fn to_words(self) -> [u32; REGS_WORDS] {
        [
            self.pc,
            self.lr,
            self.xpsr,
            self.sp,
            self.cfsr,
            self.hfsr,
            self.mmfar,
            self.bfar,
            self.exc_return,
        ]
    }

    fn from_words(w: &[u32; REGS_WORDS]) -> Self {
        CortexMRegs {
            pc: w[0],
            lr: w[1],
            xpsr: w[2],
            sp: w[3],
            cfsr: w[4],
            hfsr: w[5],
            mmfar: w[6],
            bfar: w[7],
            exc_return: w[8],
        }
    }
}

/// Stack words kept in a record (D6 of the device contract).
pub const STACK_WORDS: usize = 32;
/// Longest message kept, in bytes (the bound of `ResetRecord::message`).
pub const MESSAGE_LEN: usize = 128;
/// Longest build id kept, in bytes (sha1 is 20, sha256 32).
pub const BUILD_ID_LEN: usize = 32;
const REGS_WORDS: usize = 9;

/// "BCR" + layout version 2. A new layout changes the last byte, so an old record is dropped, never misread.
const MAGIC: u32 = 0xB0C2_5202;

/// The record as it lies in RAM. All fields are plain words or bytes, so any bit pattern is a valid value; validity
/// is decided by [Record::is_valid] (magic, CRC, bounds).
#[derive(Clone, Copy)]
#[repr(C)]
pub struct Record {
    magic: u32,
    boot_count: u32,
    /// Cause of the last reset, as `boot` decided it.
    last_cause: u32,
    /// [Cause] of the stored crash (`Fault`, `Panic`), [NO_CRASH] when there is none.
    crash: u32,
    /// [PENDING_CRASH]: the crash was captured and no boot has seen it yet, it causes the coming reset.
    /// [PENDING_RESET]: `prepare_reset` set `last_cause` for the coming reset. 0: nothing pending.
    pending: u32,
    /// Uptime of the crash in ms, [NO_UPTIME] when unknown.
    uptime_ms_lo: u32,
    uptime_ms_hi: u32,
    /// Nonzero when `regs` holds a fault's registers.
    regs_valid: u32,
    regs: [u32; REGS_WORDS],
    stack_len: u32,
    stack: [u32; STACK_WORDS],
    build_id_len: u32,
    build_id: [u8; BUILD_ID_LEN],
    message_len: u32,
    message: [u8; MESSAGE_LEN],
    /// Boots since the stored crash was saved: 0 when the last reset was the crash.
    crash_age: u32,
    crc: u32,
}

const NO_CRASH: u32 = u32::MAX;
const PENDING_CRASH: u32 = 1;
const PENDING_RESET: u32 = 2;
const NO_UPTIME: u64 = u64::MAX;

impl Record {
    /// An empty record: no crash, boot counter 0, sealed.
    pub const fn empty() -> Self {
        Record {
            magic: MAGIC,
            boot_count: 0,
            last_cause: Cause::Undetermined as u32,
            crash: NO_CRASH,
            pending: 0,
            uptime_ms_lo: u32::MAX,
            uptime_ms_hi: u32::MAX,
            regs_valid: 0,
            regs: [0; REGS_WORDS],
            stack_len: 0,
            stack: [0; STACK_WORDS],
            build_id_len: 0,
            build_id: [0; BUILD_ID_LEN],
            message_len: 0,
            message: [0; MESSAGE_LEN],
            crash_age: 0,
            crc: 0,
        }
        .sealed()
    }

    const fn sealed(mut self) -> Self {
        self.crc = crc32(self.body());
        self
    }

    /// Bytes covered by the CRC: everything before it.
    const fn body(&self) -> &[u8] {
        // SAFETY: repr(C) of u32 and u8 arrays whose sizes are multiples of 4: no padding, every byte initialised
        let all = unsafe {
            core::slice::from_raw_parts(self as *const Self as *const u8, size_of::<Self>())
        };
        all.split_at(size_of::<Self>() - 4).0
    }

    fn seal(&mut self) {
        self.crc = crc32(self.body());
    }

    /// Magic, CRC and every length and enum value in range: garbage after power-on is never taken for a record.
    pub fn is_valid(&self) -> bool {
        self.magic == MAGIC
            && self.crc == crc32(self.body())
            && Cause::from_u32(self.last_cause).is_some()
            && (self.crash == NO_CRASH || Cause::from_u32(self.crash).is_some())
            && self.stack_len as usize <= STACK_WORDS
            && self.build_id_len as usize <= BUILD_ID_LEN
            && self.message_len as usize <= MESSAGE_LEN
            && core::str::from_utf8(&self.message[..self.message_len as usize]).is_ok()
    }

    /// At boot: keep a valid record, start a new one after power loss or when it is invalid, count the boot and
    /// decide the cause of the last reset from the hardware's `hw` and a pending crash. Returns that cause.
    pub fn on_boot(&mut self, hw: Cause) -> Cause {
        if hw.power_lost() || !self.is_valid() {
            *self = Record::empty();
        }
        // a crash handler or `prepare_reset` reset the device: that cause beats what the register says
        // (Software, or stale flags on parts whose reset-cause register doesn't record SYSRESETREQ)
        let cause = self.pending_cause().unwrap_or(hw);
        if self.pending == PENDING_CRASH {
            self.crash_age = 0;
        } else {
            self.crash_age = self.crash_age.saturating_add(1);
        }
        self.pending = 0;
        self.boot_count = self.boot_count.saturating_add(1);
        self.last_cause = cause as u32;
        self.seal();
        cause
    }

    /// Store a crash, unless one was already captured since the last boot (the first one is the cause; a fault in
    /// the panic path doesn't overwrite the panic).
    ///
    /// `fill` writes the details into a [CrashWriter]. Called from fault and panic handlers, sealed at the end.
    pub fn capture(
        &mut self,
        cause: Cause,
        uptime_ms: Option<u64>,
        build_id: &[u8],
        fill: impl FnOnce(&mut CrashWriter<'_>),
    ) {
        if self.is_valid() && self.pending == PENDING_CRASH {
            return;
        }
        if !self.is_valid() {
            // no boot() ran (or RAM was overwritten): still keep the crash
            *self = Record::empty();
        }
        let uptime = uptime_ms.unwrap_or(NO_UPTIME);
        self.crash = cause as u32;
        self.pending = PENDING_CRASH;
        self.uptime_ms_lo = uptime as u32;
        self.uptime_ms_hi = (uptime >> 32) as u32;
        self.regs_valid = 0;
        self.stack_len = 0;
        self.message_len = 0;
        let n = build_id.len().min(BUILD_ID_LEN);
        self.build_id[..n].copy_from_slice(&build_id[..n]);
        self.build_id_len = n as u32;
        fill(&mut CrashWriter { record: self });
        self.seal();
    }

    /// Mark the coming software reset as `cause` (e.g. [Cause::Updated] before resetting into a new image), unless
    /// a crash is pending. No crash data is stored.
    pub fn prepare_reset(&mut self, cause: Cause) {
        if !self.is_valid() {
            *self = Record::empty();
        }
        if self.pending == PENDING_CRASH {
            return;
        }
        // keep an older crash: only the cause of the coming reset changes
        self.last_cause = cause as u32;
        self.pending = PENDING_RESET;
        self.seal();
    }

    /// Cause of the coming reset when a handler or [Record::prepare_reset] set one.
    fn pending_cause(&self) -> Option<Cause> {
        match self.pending {
            PENDING_CRASH => Cause::from_u32(self.crash),
            PENDING_RESET => Cause::from_u32(self.last_cause),
            _ => None,
        }
    }

    /// Forget the crash and zero the boot counter.
    pub fn clear(&mut self) {
        *self = Record::empty();
    }

    /// The decoded record, with `cause` the cause of the last reset ([Record::on_boot]'s result).
    pub fn view(&self) -> View<'_> {
        let crash = (self.crash != NO_CRASH).then(|| {
            let uptime = (self.uptime_ms_hi as u64) << 32 | self.uptime_ms_lo as u64;
            Crash {
                cause: Cause::from_u32(self.crash).unwrap_or(Cause::Undetermined),
                uptime_ms: (uptime != NO_UPTIME).then_some(uptime),
                regs: (self.regs_valid != 0).then(|| CortexMRegs::from_words(&self.regs)),
                stack: &self.stack[..(self.stack_len as usize).min(STACK_WORDS)],
                message: core::str::from_utf8(
                    &self.message[..(self.message_len as usize).min(MESSAGE_LEN)],
                )
                .unwrap_or(""),
                build_id: &self.build_id[..(self.build_id_len as usize).min(BUILD_ID_LEN)],
                boots_since: self.crash_age,
            }
        });
        View {
            cause: Cause::from_u32(self.last_cause).unwrap_or(Cause::Undetermined),
            boot_count: self.boot_count,
            crash,
        }
    }
}

/// Fills in a crash, see [Record::capture].
pub struct CrashWriter<'a> {
    record: &'a mut Record,
}

impl CrashWriter<'_> {
    /// Registers of a Cortex-M fault.
    pub fn regs(&mut self, regs: CortexMRegs) {
        self.record.regs = regs.to_words();
        self.record.regs_valid = 1;
    }

    /// Append a stack word; returns false when the record is full.
    pub fn push_stack(&mut self, word: u32) -> bool {
        let n = self.record.stack_len as usize;
        if n >= STACK_WORDS {
            return false;
        }
        self.record.stack[n] = word;
        self.record.stack_len += 1;
        true
    }
}

/// Appends to the message, cut at [MESSAGE_LEN] on a char boundary; never fails.
impl core::fmt::Write for CrashWriter<'_> {
    fn write_str(&mut self, s: &str) -> core::fmt::Result {
        let r = &mut *self.record;
        let at = r.message_len as usize;
        let room = MESSAGE_LEN - at;
        let mut n = s.len().min(room);
        while !s.is_char_boundary(n) {
            n -= 1;
        }
        r.message[at..at + n].copy_from_slice(&s.as_bytes()[..n]);
        r.message_len += n as u32;
        Ok(())
    }
}

/// What a [Record] says, borrowed from it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct View<'a> {
    /// Cause of the last reset.
    pub cause: Cause,
    /// Boots since power-on or the last clear.
    pub boot_count: u32,
    /// The stored crash, kept until cleared; it caused the last reset when `cause` is its cause.
    pub crash: Option<Crash<'a>>,
}

/// A crash saved by a fault or panic handler.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Crash<'a> {
    /// [Cause::Fault] or [Cause::Panic].
    pub cause: Cause,
    /// Time since boot at the crash.
    pub uptime_ms: Option<u64>,
    /// Registers at a fault, `None` for a panic.
    pub regs: Option<CortexMRegs>,
    /// Stack words from the stack pointer at the crash, innermost first.
    pub stack: &'a [u32],
    /// Panic message (with its location) or fault text.
    pub message: &'a str,
    /// Build id of the firmware that crashed, empty when it had none.
    pub build_id: &'a [u8],
    /// Boots since the crash was saved: 0 when the last reset was the crash, 1 after one normal reset, and so on.
    pub boots_since: u32,
}

/// An owned copy of the record taken at boot, for a `LastReset` server: reading it needs no access to the RAM area
/// a fault handler may write.
#[derive(Clone)]
pub struct Snapshot {
    record: Record,
}

impl Snapshot {
    /// A snapshot of `record`, as `boot` takes it.
    pub fn new(record: Record) -> Self {
        Snapshot { record }
    }

    /// Cause of the last reset.
    pub fn cause(&self) -> Cause {
        self.view().cause
    }

    /// Boots since power-on or the last clear.
    pub fn boot_count(&self) -> u32 {
        self.record.boot_count
    }

    /// The decoded record.
    pub fn view(&self) -> View<'_> {
        self.record.view()
    }

    /// Forget the crash and zero the boot counter here; `cortex_m::clear` also clears the RAM record.
    pub fn clear_snapshot(&mut self) {
        let cause = self.record.last_cause;
        self.record = Record::empty();
        // the last reset's cause stays what it was
        self.record.last_cause = cause;
        self.record.seal();
    }
}

/// A short text for a Cortex-M fault: `HardFault` and the set fault status bits (`HardFault FORCED PRECISERR
/// BFARVALID`), written into the record's message. Zero registers (ARMv6-M) give just `HardFault`.
pub fn describe_fault(w: &mut impl core::fmt::Write, cfsr: u32, hfsr: u32) {
    const CFSR: [(u32, &str); 20] = [
        (0, "IACCVIOL"),
        (1, "DACCVIOL"),
        (3, "MUNSTKERR"),
        (4, "MSTKERR"),
        (5, "MLSPERR"),
        (7, "MMARVALID"),
        (8, "IBUSERR"),
        (9, "PRECISERR"),
        (10, "IMPRECISERR"),
        (11, "UNSTKERR"),
        (12, "STKERR"),
        (13, "LSPERR"),
        (15, "BFARVALID"),
        (16, "UNDEFINSTR"),
        (17, "INVSTATE"),
        (18, "INVPC"),
        (19, "NOCP"),
        (20, "STKOF"),
        (24, "UNALIGNED"),
        (25, "DIVBYZERO"),
    ];
    const HFSR: [(u32, &str); 3] = [(1, "VECTTBL"), (30, "FORCED"), (31, "DEBUGEVT")];
    let _ = w.write_str("HardFault");
    let set = HFSR.iter().filter(|(b, _)| hfsr & (1 << b) != 0);
    for (_, name) in set.chain(CFSR.iter().filter(|(b, _)| cfsr & (1 << b) != 0)) {
        let _ = write!(w, " {name}");
    }
}

/// CRC-32 (IEEE, reflected, as zlib), bitwise: no table in flash, a few hundred bytes run once per boot or crash.
pub const fn crc32(data: &[u8]) -> u32 {
    let mut crc = 0xFFFF_FFFFu32;
    let mut i = 0;
    while i < data.len() {
        crc ^= data[i] as u32;
        let mut b = 0;
        while b < 8 {
            crc = if crc & 1 != 0 {
                (crc >> 1) ^ 0xEDB8_8320
            } else {
                crc >> 1
            };
            b += 1;
        }
        i += 1;
    }
    !crc
}

#[cfg(test)]
mod tests;
