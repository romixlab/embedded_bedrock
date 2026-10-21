#![cfg_attr(not(test), no_std)]
//! Status LED animation for bedrock firmware (DEV-12): a [Mode] per LED (off, on, breath, blink) with eased fades,
//! shown through one indication state ([Leds]): which LEDs are visible, one peak brightness for all, a colour per RGB
//! LED, and an identify blink that overrides everything for a while.
//!
//! The layers, each usable on its own:
//! - [Animation] steps a [Mode] every [TICK_MS] and gives its perceptual level (0..=65535); [scale] applies a peak
//!   brightness, [duty] the gamma. Plain u32 math, tested on the host.
//! - [Leds] is the state of N LEDs under one indication: the levels to show this tick, HAL-free.
//! - [Channel] is one PWM output; [Pwm] adapts any embedded-hal [SetDutyCycle](embedded_hal::pwm::SetDutyCycle)
//!   (active-low option).
//! - `Indication` (feature `embassy`) is the shared state in a `static` and the task loop; the board's closure
//!   writes each tick's levels to its channels.
//! - `indication` (feature `ww-indication`) is the bookkeeping behind a `ww_indication::Indication` server.
//!
//! All math is u32 except one 64-bit multiply in [ease]: Cortex-M0+ has no hardware divide.

#[cfg(feature = "embassy")]
mod driver;
#[cfg(feature = "ww-indication")]
pub mod indication;
mod leds;
mod output;

#[cfg(feature = "embassy")]
pub use driver::Indication;
pub use leds::Leds;
pub use output::{Channel, Pwm};

/// Animation update period.
pub const TICK_MS: u32 = 10;
/// Blink edge duration, shortened to fit when on/off time is shorter.
pub const BLINK_FADE_MS: u32 = 100;
/// Default peak brightness, perceptual (gamma corrected): 0 = off, 255 = full PWM duty. 128 is a quarter of the
/// duty: well visible on a desk without glaring.
pub const DEFAULT_PEAK: u8 = 128;
/// Identify: full brightness, on and off this long.
pub const IDENTIFY_BLINK_MS: u32 = 150;
/// Full perceptual level.
pub const FULL: u32 = 65535;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum Mode {
    Off,
    On,
    /// Fade 0 -> peak -> 0, once per `period_ms`.
    Breath {
        period_ms: u32,
    },
    /// Eased fade up at the start of `on_ms`, eased fade down at the start of `off_ms`.
    Blink {
        on_ms: u32,
        off_ms: u32,
    },
}

/// A running [Mode]: where in its cycle it is.
#[derive(Clone, Copy, Debug)]
pub struct Animation {
    mode: Mode,
    /// Breath: position in the cycle, one full u32 wrap = one period.
    /// Blink: ms since the start of the current on time.
    phase: u32,
    /// Breath: phase increment per tick. Blink: edge fade duration in ms (>= 1).
    step: u32,
}

impl Animation {
    /// Starts `mode` from dark (breath and blink begin with the fade up).
    pub fn new(mode: Mode) -> Self {
        let mode = match mode {
            Mode::Blink { on_ms: 0, .. } => Mode::Off,
            Mode::Blink { off_ms: 0, .. } => Mode::On,
            m => m,
        };
        let step = match mode {
            Mode::Breath { period_ms } => u32::MAX / period_ms.max(TICK_MS) * TICK_MS,
            Mode::Blink { on_ms, off_ms } => BLINK_FADE_MS.min(on_ms).min(off_ms).max(1),
            _ => 0,
        };
        Self {
            mode,
            phase: 0,
            step,
        }
    }

    /// The identify blink.
    pub fn identify() -> Self {
        Self::new(Mode::Blink {
            on_ms: IDENTIFY_BLINK_MS,
            off_ms: IDENTIFY_BLINK_MS,
        })
    }

    /// Animation shape, perceptual 0..=65535, before the peak brightness.
    pub fn level(&self) -> u32 {
        match self.mode {
            Mode::Off => 0,
            Mode::On => FULL,
            Mode::Breath { .. } => {
                // triangle 0 -> 65535 -> 0
                let p = self.phase >> 15; // 0..131072
                let x = if p < 65536 { p } else { 131071 - p };
                ease(x)
            }
            Mode::Blink { on_ms, .. } => {
                let fade = self.step;
                let x = if self.phase < on_ms {
                    self.phase.min(fade) * FULL / fade
                } else {
                    FULL - (self.phase - on_ms).min(fade) * FULL / fade
                };
                ease(x)
            }
        }
    }

    /// Whether the level changes over time (the task can sleep otherwise).
    pub fn animated(&self) -> bool {
        matches!(self.mode, Mode::Breath { .. } | Mode::Blink { .. })
    }

    /// One [TICK_MS] on.
    pub fn advance(&mut self) {
        match self.mode {
            Mode::Blink { on_ms, off_ms } => {
                self.phase = self.phase.saturating_add(TICK_MS);
                let period = on_ms.saturating_add(off_ms);
                if self.phase >= period {
                    // period > 0: new() turns a blink with no on or off time into Off or On
                    self.phase = self.phase.checked_rem(period).unwrap_or(0);
                }
            }
            _ => self.phase = self.phase.wrapping_add(self.step),
        }
    }
}

/// Smoothstep 3x^2 - 2x^3 on Q16 (0..=65535), for soft starts and stops.
pub fn ease(x: u32) -> u32 {
    // x^2 * (3 - 2x) in 64 bits (< 2^50), so it rises monotonically to the last count (UMULL on M3 and up,
    // __aeabi_lmul on the M0+)
    let x = x.min(FULL) as u64;
    ((x * x * (3 * 65536 - 2 * x)) >> 32) as u32
}

/// A perceptual level (0..=65535) scaled by a peak brightness (0..=255).
pub fn scale(level: u32, peak: u8) -> u32 {
    level * peak as u32 / 255
}

/// Maps a perceptual level (0..=65535) to PWM duty with gamma 2. `max_duty` must be <= 65536.
pub fn duty(level: u32, max_duty: u32) -> u32 {
    let l2 = (level * level) >> 16; // 0..=65534
    (l2 * max_duty + 0x8000) >> 16 // rounded: full level gives max_duty (within 1 count at 65536)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn levels(mut a: Animation, ticks: usize) -> Vec<u32> {
        (0..ticks)
            .map(|_| {
                let l = a.level();
                a.advance();
                l
            })
            .collect()
    }

    #[test]
    fn ease_ends_and_middle() {
        assert_eq!(ease(0), 0);
        assert!(ease(65535) >= 65533);
        assert!(ease(32768).abs_diff(32768) < 8);
        // soft start: slower than linear at first
        assert!(ease(6553) < 6553 / 2);
    }

    #[test]
    fn ease_is_monotonic() {
        let mut last = 0;
        for x in 0..=65535 {
            let e = ease(x);
            assert!(e >= last, "ease({x}) = {e} < {last}");
            last = e;
        }
    }

    #[test]
    fn gamma_duty() {
        assert_eq!(duty(0, 65536), 0);
        assert!(duty(65535, 65536) >= 65533);
        // half perceptual is a quarter of the duty
        assert!(duty(32768, 65536).abs_diff(16384) < 2);
        assert_eq!(duty(65535, 1000), 1000);
        // never above max_duty
        for max in [1, 255, 1000, 64000, 65535, 65536] {
            assert!(duty(FULL, max) <= max);
        }
    }

    #[test]
    fn default_peak_is_calm() {
        // moderate peak: a quarter of the duty at most
        let top = duty(scale(FULL, DEFAULT_PEAK), 65536);
        assert!((16000..17000).contains(&top), "{top}");
    }

    #[test]
    fn breath_cycle() {
        let a = Animation::new(Mode::Breath { period_ms: 1000 });
        let l = levels(a, 101);
        assert_eq!(l[0], 0);
        let (peak_at, peak) = l.iter().enumerate().max_by_key(|(_, l)| **l).unwrap();
        assert!(peak > &65000);
        assert!((49..=51).contains(&peak_at), "{peak_at}");
        assert!(l[100] < 100, "back to dark after one period: {}", l[100]);
        // rises then falls
        assert!(l[..50].windows(2).all(|w| w[0] <= w[1]));
        assert!(l[51..100].windows(2).all(|w| w[0] >= w[1]));
    }

    #[test]
    fn breath_period_shorter_than_a_tick() {
        // cut to one tick per period instead of dividing by zero
        let a = Animation::new(Mode::Breath { period_ms: 0 });
        assert!(a.animated());
        assert_eq!(levels(a, 3), [0, 0, 0]);
    }

    #[test]
    fn blink_eased_edges() {
        let a = Animation::new(Mode::Blink {
            on_ms: 500,
            off_ms: 500,
        });
        let l = levels(a, 101);
        assert_eq!(l[0], 0);
        // 100 ms fade up, full until the off time
        assert!(l[5] > 20000 && l[5] < 45000);
        assert!(l[10] >= 65533 && l[49] >= 65533);
        // fade down from 500 ms, dark by 600 ms, on again at 1000 ms
        assert!(l[55] > 20000 && l[55] < 45000);
        assert_eq!(l[60], 0);
        assert_eq!(l[99], 0);
        assert_eq!(l[100], 0);
    }

    #[test]
    fn short_blink_fades_fit() {
        let a = Animation::identify();
        let l = levels(a, 30);
        // 150 ms on and off: 100 ms edges
        assert!(l[10] >= 65533);
        assert_eq!(l[25], 0);
        // 40 ms on: the edges shrink to 40 ms
        let l = levels(
            Animation::new(Mode::Blink {
                on_ms: 40,
                off_ms: 500,
            }),
            5,
        );
        assert!(l[4] >= 65533, "{l:?}");
    }

    #[test]
    fn degenerate_blinks() {
        let off = Animation::new(Mode::Blink {
            on_ms: 0,
            off_ms: 100,
        });
        assert!(!off.animated());
        assert_eq!(off.level(), 0);
        let on = Animation::new(Mode::Blink {
            on_ms: 100,
            off_ms: 0,
        });
        assert!(!on.animated());
        assert_eq!(on.level(), 65535);
        // huge times don't overflow
        let mut big = Animation::new(Mode::Blink {
            on_ms: u32::MAX,
            off_ms: u32::MAX,
        });
        big.advance();
        assert!(big.level() > 0);
    }

    #[test]
    fn peak_scaling() {
        assert_eq!(scale(65535, 255), 65535);
        assert_eq!(scale(65535, 0), 0);
        assert_eq!(scale(65535, 128), 32896);
    }
}
