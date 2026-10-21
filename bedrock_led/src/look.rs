//! The bedrock look (DEV-15): what a status LED shows for what it means, the same on every board. A board says
//! [OK], [BUSY], [WARN], [FAULT], [ALERT] (in a colour from [colour] where it has an RGB LED) and gets the same
//! animation as Donguru, rePort and B135B; nothing visual is chosen per board.
//!
//! Timing: every period divides [CYCLE_MS], so all looks start a cycle together every 3 s, and with shared time
//! (DEV-14) every board's cycles start on the same UTC multiples of it: a breath on one board and a blink on another
//! are in step too. Fades are [BLINK_FADE_MS](crate::BLINK_FADE_MS), the peak brightness
//! [DEFAULT_PEAK](crate::DEFAULT_PEAK) and the per-brightness peaks `indication::PEAKS`.
//!
//! A board-specific look is fine where there is a real reason (a light pipe that needs another peak, a single LED
//! that must tell two states apart); say the reason in a comment next to it.

use crate::{IDENTIFY_BLINK_MS, Mode};

/// The common cycle every look's period divides, ms.
pub const CYCLE_MS: u32 = 3000;
/// A calm breath: running, all good.
pub const BREATH_MS: u32 = CYCLE_MS;
/// A quicker breath: working on something, or something wants a look.
pub const FAST_BREATH_MS: u32 = CYCLE_MS / 2;
/// A fast blink, on and off this long: a fault that needs someone.
pub const ALERT_BLINK_MS: u32 = 250;
/// A short flash this long once per [HEARTBEAT_PERIOD_MS]: alive, for a LED next to one that breathes.
pub const HEARTBEAT_ON_MS: u32 = 300;
pub const HEARTBEAT_PERIOD_MS: u32 = CYCLE_MS / 2;

/// The status colours, named by meaning, for RGB LEDs (`[red, green, blue]`, full scale; the peak brightness
/// scales them).
pub mod colour {
    /// Running, all good.
    pub const OK: [u8; 3] = [0, 255, 0];
    /// Something wants a look, still working.
    pub const WARN: [u8; 3] = [255, 110, 0];
    /// Not working.
    pub const FAULT: [u8; 3] = [255, 0, 0];
    /// Storage (a card, a disk) present and ready.
    pub const STORAGE: [u8; 3] = [0, 90, 255];
}

/// What one LED shows: an animation in a colour (mono LEDs show only the [Mode]).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub struct Look {
    pub mode: Mode,
    pub colour: [u8; 3],
}

/// Dark.
pub const OFF: Look = Look {
    mode: Mode::Off,
    colour: [0; 3],
};
/// Running, all good: a calm breath.
pub const OK: Look = Look {
    mode: Mode::Breath {
        period_ms: BREATH_MS,
    },
    colour: colour::OK,
};
/// Working on something (an update, a transfer): a quicker breath.
pub const BUSY: Look = Look {
    mode: Mode::Breath {
        period_ms: FAST_BREATH_MS,
    },
    colour: colour::OK,
};
/// Still working, but something wants a look (a bus in error-passive): a quicker breath in amber.
pub const WARN: Look = Look {
    mode: Mode::Breath {
        period_ms: FAST_BREATH_MS,
    },
    colour: colour::WARN,
};
/// Not working (a card that didn't come up): steady red.
pub const FAULT: Look = Look {
    mode: Mode::On,
    colour: colour::FAULT,
};
/// Not working and needs someone now (a bus off): a fast red blink.
pub const ALERT: Look = Look {
    mode: Mode::Blink {
        on_ms: ALERT_BLINK_MS,
        off_ms: ALERT_BLINK_MS,
    },
    colour: colour::FAULT,
};
/// Alive: a short flash, for a second LED next to one that breathes [OK].
pub const HEARTBEAT: Look = Look {
    mode: Mode::Blink {
        on_ms: HEARTBEAT_ON_MS,
        off_ms: HEARTBEAT_PERIOD_MS - HEARTBEAT_ON_MS,
    },
    colour: colour::OK,
};
/// Identify ("which board is it?"): full brightness, fast, overriding everything (`Leds::identify`).
pub const IDENTIFY: Mode = Mode::Blink {
    on_ms: IDENTIFY_BLINK_MS,
    off_ms: IDENTIFY_BLINK_MS,
};

impl Look {
    /// The same animation in another colour: [OK] in [colour::STORAGE] for a card that is ready.
    pub const fn in_colour(self, colour: [u8; 3]) -> Look {
        Look {
            mode: self.mode,
            colour,
        }
    }

    /// The look under a `ww_indication` mode: `AllOff` is dark, `OnlyEssential` keeps essential looks (faults) and
    /// shows them steady, a mode from a newer host acts as `Normal`.
    #[cfg(feature = "ww-indication")]
    pub fn under(self, global: crate::indication::Mode, essential: bool) -> Look {
        use crate::indication::Mode as Global;
        match global {
            Global::AllOff => OFF,
            Global::OnlyEssential if essential => Look {
                mode: Mode::On,
                ..self
            },
            Global::OnlyEssential => OFF,
            _ => self,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn period_ms(mode: Mode) -> Option<u32> {
        match mode {
            Mode::Breath { period_ms } => Some(period_ms),
            Mode::Blink { on_ms, off_ms } => Some(on_ms + off_ms),
            _ => None,
        }
    }

    #[test]
    fn every_period_divides_the_cycle() {
        for look in [OFF, OK, BUSY, WARN, FAULT, ALERT, HEARTBEAT] {
            if let Some(p) = period_ms(look.mode) {
                assert_eq!(CYCLE_MS % p, 0, "{look:?}");
            }
        }
        assert_eq!(CYCLE_MS % period_ms(IDENTIFY).unwrap(), 0);
    }

    #[test]
    fn looks_are_told_apart() {
        let all = [OFF, OK, BUSY, WARN, FAULT, ALERT, HEARTBEAT];
        for (i, a) in all.iter().enumerate() {
            for b in &all[i + 1..] {
                assert_ne!(a, b);
            }
        }
        assert_eq!(OK.in_colour(colour::STORAGE).mode, OK.mode);
    }

    #[cfg(feature = "ww-indication")]
    #[test]
    fn under_the_global_mode() {
        use crate::indication::Mode as Global;
        assert_eq!(OK.under(Global::Normal, false), OK);
        assert_eq!(ALERT.under(Global::AllOff, true), OFF);
        assert_eq!(OK.under(Global::OnlyEssential, false), OFF);
        assert_eq!(
            ALERT.under(Global::OnlyEssential, true),
            Look {
                mode: Mode::On,
                colour: colour::FAULT
            }
        );
    }
}
