//! RP2040 / RP235x reset reasons (`CHIP_RESET`, `WATCHDOG.REASON`) to a [Cause].
//!
//! RP235x: `POWMAN.CHIP_RESET`; RP2040: `VREG_AND_CHIP_RESET.CHIP_RESET` (`HAD_POR`, `HAD_RUN`, `HAD_PSM_RESTART`).
//! The firmware reads both registers and fills [ResetFlags] (the bedrock template generates that code).
//!
//! `CHIP_RESET` records the last *chip-level* reset and its `HAD_*` bits are read-only: nothing clears them but the
//! next chip-level reset. An Arm `SYSRESETREQ` (`SCB::sys_reset`, the crash handler's reset) is not one: "This resets
//! only the core on which SYSRESETREQ is asserted, and not the wider system" (RP2350 datasheet, `AIRCR.SYSRESETREQ`;
//! RP2040 datasheet 2.4.2: "The NVIC only resets the Cortex-M0+ processor core"). So after a power-on, `HAD_POR`
//! stays set across every crash reset, and read alone it made every reset look like a power loss (ROB-14).
//!
//! The firmware tells a fresh `CHIP_RESET` from a stale one with a marker in a watchdog scratch register
//! ([CHIP_RESET_SCRATCH]): every chip-level reset clears the watchdog scratch registers (RP2350 datasheet 7.3.2,
//! "All chip-level reset sources in the table also reset the system watchdog peripheral. This includes watchdog
//! scratch registers SCRATCH0 -> SCRATCH7"; RP2040: "The watchdog is reset by rst_n_run", POR, brown-out and RUN),
//! while a core reset leaves them. At boot it reads the marker ([chip_reset_fresh]), writes [CHIP_RESET_SEEN] and
//! uses `CHIP_RESET` only when it was fresh. `WATCHDOG.REASON` is read either way: a watchdog reset that doesn't
//! reach chip level keeps the scratch registers but sets `REASON`, and on RP2350 a `SYSRESETREQ` clears it.
//!
//! The same register carries [Cause::Updated] through the boot ROM's BOOTSEL: the ROM erases all SRAM before
//! entering it (RP2350 datasheet 5.2.2, "Prepare for BOOTSEL: Erase SRAM0 through SRAM9, XIP cache and USB RAM to
//! all-zeroes"), so the RAM record can't, but its reboots keep the watchdog scratch registers (seen on a B200, RP2350
//! A4: the marker and `HAD_POR` from the power-up both still there after a PICOBOOT flash). Right before
//! `reset_to_usb_boot` the firmware writes [UPDATE_PENDING] there; the next boot reports [Cause::Updated]
//! ([update_pending]), whether or not an image was written in between.
//!
//! With no flag set the cause is [Cause::Software]; a crash handler's reset is recognised from the record itself.

use crate::Cause;

/// The watchdog scratch register holding the [CHIP_RESET_SEEN] marker. The bootrom uses SCRATCH4..7 (watchdog
/// boot vector, `reboot()`) and RP2350's `reboot()` its parameters in SCRATCH2..3; SCRATCH0 is the one examples
/// reach for first.
pub const CHIP_RESET_SCRATCH: usize = 1;

/// Written to [CHIP_RESET_SCRATCH] at boot: `CHIP_RESET` has been read since the last chip-level reset.
pub const CHIP_RESET_SEEN: u32 = 0xB0C7_5EE4;

/// Written to [CHIP_RESET_SCRATCH] right before entering BOOTSEL for an update: [CHIP_RESET_SEEN], and the next
/// boot is [Cause::Updated].
pub const UPDATE_PENDING: u32 = 0xB0C7_0FDA;

/// Whether `CHIP_RESET` tells about the reset that just happened: `scratch` is the value of [CHIP_RESET_SCRATCH]
/// at boot, before writing [CHIP_RESET_SEEN] to it. False after a core reset (`SYSRESETREQ`) or the boot ROM's
/// reboots: then `CHIP_RESET` still holds an older chip-level reset and the firmware reads it as all zeros.
pub fn chip_reset_fresh(scratch: u32) -> bool {
    scratch != CHIP_RESET_SEEN && scratch != UPDATE_PENDING
}

/// Whether the firmware entered BOOTSEL for an update before this boot ([UPDATE_PENDING]); `scratch` as for
/// [chip_reset_fresh].
pub fn update_pending(scratch: u32) -> bool {
    scratch == UPDATE_PENDING
}

/// The reset reasons of an RP2040 / RP235x, by meaning.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ResetFlags {
    /// [update_pending]: the firmware went into BOOTSEL for an update.
    pub updated: bool,
    /// `HAD_POR`.
    pub power_on: bool,
    /// `HAD_BOR` (RP235x).
    pub brownout: bool,
    /// `HAD_GLITCH_DETECT` (RP235x): a supply glitch.
    pub glitch: bool,
    /// `HAD_RUN_LOW` (RP235x), `HAD_RUN` (RP2040).
    pub run_pin: bool,
    /// A debugger reset: `HAD_DP_RESET_REQ`, `HAD_RESCUE`, `HAD_HZD_SYS_RESET_REQ` (RP235x), `HAD_PSM_RESTART`
    /// (RP2040).
    pub debugger: bool,
    /// `WATCHDOG.REASON.FORCE`: firmware triggered the watchdog to reset (a deliberate reboot).
    pub watchdog_force: bool,
    /// `WATCHDOG.REASON.TIMER`, or any `HAD_WATCHDOG_RESET_*` (RP235x).
    pub watchdog_timer: bool,
}

impl ResetFlags {
    /// The cause, the most specific reason first.
    pub fn cause(&self) -> Cause {
        if self.updated {
            // the marker survives no chip-level reset, so no other flag is fresh with it
            Cause::Updated
        } else if self.power_on {
            Cause::PowerOn
        } else if self.brownout || self.glitch {
            Cause::Brownout
        } else if self.run_pin {
            Cause::Pin
        } else if self.debugger {
            Cause::Undetermined
        } else if self.watchdog_force {
            Cause::Software
        } else if self.watchdog_timer {
            Cause::Watchdog
        } else {
            // SYSRESETREQ leaves no trace
            Cause::Software
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn causes() {
        let f = |f: ResetFlags| f.cause();
        assert_eq!(
            f(ResetFlags {
                power_on: true,
                run_pin: true,
                ..Default::default()
            }),
            Cause::PowerOn
        );
        assert_eq!(
            f(ResetFlags {
                glitch: true,
                ..Default::default()
            }),
            Cause::Brownout
        );
        assert_eq!(
            f(ResetFlags {
                run_pin: true,
                ..Default::default()
            }),
            Cause::Pin
        );
        assert_eq!(
            f(ResetFlags {
                debugger: true,
                ..Default::default()
            }),
            Cause::Undetermined
        );
        // a forced watchdog reset also sets the timer-side HAD_WATCHDOG_RESET_* bits
        assert_eq!(
            f(ResetFlags {
                watchdog_force: true,
                watchdog_timer: true,
                ..Default::default()
            }),
            Cause::Software
        );
        assert_eq!(
            f(ResetFlags {
                watchdog_timer: true,
                ..Default::default()
            }),
            Cause::Watchdog
        );
        assert_eq!(f(ResetFlags::default()), Cause::Software);
    }

    #[test]
    fn marker() {
        // power-on or a chip-level reset: the scratch registers read zero
        assert!(chip_reset_fresh(0));
        // the boot before wrote the marker and only the core was reset since
        assert!(!chip_reset_fresh(CHIP_RESET_SEEN));
        assert!(!update_pending(CHIP_RESET_SEEN));
        // entered BOOTSEL for an update: the ROM's reboots keep the scratch registers
        assert!(!chip_reset_fresh(UPDATE_PENDING));
        assert!(update_pending(UPDATE_PENDING));
        assert!(!update_pending(0));
        // the ROM's reboot sets WATCHDOG.REASON.TIMER
        assert_eq!(
            ResetFlags {
                updated: true,
                watchdog_timer: true,
                ..Default::default()
            }
            .cause(),
            Cause::Updated
        );
    }
}
