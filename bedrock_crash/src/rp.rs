//! RP2040 / RP235x reset reasons (`CHIP_RESET`, `WATCHDOG.REASON`) to a [Cause].
//!
//! RP235x: `POWMAN.CHIP_RESET`; RP2040: `VREG_AND_CHIP_RESET.CHIP_RESET` (`HAD_POR`, `HAD_RUN`, `HAD_PSM_RESTART`).
//! The firmware reads both registers and fills [ResetFlags] (the bedrock template generates that code).
//!
//! Neither chip records an Arm `SYSRESETREQ` (`SCB::sys_reset`) in `CHIP_RESET`: with no flag set the cause is
//! [Cause::Software]. A crash handler's reset is recognised from the record itself, not from these flags.

use crate::Cause;

/// The reset reasons of an RP2040 / RP235x, by meaning.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ResetFlags {
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
        if self.power_on {
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
}
