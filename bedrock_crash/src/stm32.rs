//! STM32 reset flags (`RCC_CSR`, `RCC_RSR` on H5 / H7) to a [Cause].
//!
//! The flags differ per family (F4 has `PADRSTF` and `WDGRSTF`, G0 `PWRRSTF`, L4 / G4 / U5 only `BORRSTF` for a
//! power-on); the firmware reads its register and fills [ResetFlags] (the bedrock template generates that code from
//! stm32-data), then clears them with `RMVF` so the next reset starts fresh.

use crate::Cause;

/// The reset flags of an STM32, by meaning.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ResetFlags {
    /// `PORRSTF` / `PWRRSTF`, or `BORRSTF` on parts without a separate power-on flag.
    pub power_on: bool,
    /// `BORRSTF` on parts that also have a power-on flag.
    pub brownout: bool,
    /// `PINRSTF` / `PADRSTF`. Every internal reset drives NRST low too, so this is set with the others.
    pub pin: bool,
    /// `IWDGRSTF` / `WDGRSTF` / `IWDG1RSTF`.
    pub independent_watchdog: bool,
    /// `WWDGRSTF` / `WWDG1RSTF`.
    pub window_watchdog: bool,
    /// `SFTRSTF`: `SYSRESETREQ`.
    pub software: bool,
    /// `LPWRRSTF`: illegal Stop / Standby entry.
    pub low_power: bool,
    /// `OBLRSTF`: option byte loading.
    pub option_bytes: bool,
    /// `FWRSTF`: firewall.
    pub firewall: bool,
}

impl ResetFlags {
    /// The cause, the most specific flag first (`pin` last, it comes with every internal reset).
    pub fn cause(&self) -> Cause {
        if self.power_on {
            Cause::PowerOn
        } else if self.brownout {
            Cause::Brownout
        } else if self.independent_watchdog || self.window_watchdog {
            Cause::Watchdog
        } else if self.software {
            Cause::Software
        } else if self.low_power || self.option_bytes || self.firewall {
            Cause::Undetermined
        } else if self.pin {
            Cause::Pin
        } else {
            Cause::Undetermined
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn power_on_beats_pin_and_brownout() {
        let f = ResetFlags {
            power_on: true,
            brownout: true,
            pin: true,
            ..Default::default()
        };
        assert_eq!(f.cause(), Cause::PowerOn);
        let f = ResetFlags {
            brownout: true,
            pin: true,
            ..Default::default()
        };
        assert_eq!(f.cause(), Cause::Brownout);
    }

    #[test]
    fn internal_resets_come_with_pin() {
        let f = ResetFlags {
            software: true,
            pin: true,
            ..Default::default()
        };
        assert_eq!(f.cause(), Cause::Software);
        let f = ResetFlags {
            independent_watchdog: true,
            pin: true,
            ..Default::default()
        };
        assert_eq!(f.cause(), Cause::Watchdog);
        let f = ResetFlags {
            window_watchdog: true,
            software: true,
            pin: true,
            ..Default::default()
        };
        assert_eq!(f.cause(), Cause::Watchdog);
        let f = ResetFlags {
            option_bytes: true,
            pin: true,
            ..Default::default()
        };
        assert_eq!(f.cause(), Cause::Undetermined);
    }

    #[test]
    fn pin_alone_and_nothing() {
        let f = ResetFlags {
            pin: true,
            ..Default::default()
        };
        assert_eq!(f.cause(), Cause::Pin);
        assert_eq!(ResetFlags::default().cause(), Cause::Undetermined);
    }
}
