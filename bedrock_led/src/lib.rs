#![cfg_attr(not(test), no_std)]
//! Status LED animation for bedrock firmware (DEV-12): a [Mode] per LED (off, on, breath, blink) with eased fades,
//! shown through one indication state ([Leds]): which LEDs are visible, one peak brightness for all, a colour per RGB
//! LED, and an identify blink that overrides everything for a while.
//!
//! The layers, each usable on its own:
//! - [Animation] steps a [Mode] every [TICK_MS] and gives its perceptual level (0..=65535); [scale] applies a peak
//!   brightness, [duty] the gamma. Plain u32 math, tested on the host. With shared time (a host-steered or
//!   line-locked clock, bedrock_sync) [Animation::align] puts it at `t mod period` instead (DEV-14), so devices
//!   breathe in phase; without, it runs free from its last mode change as before.
//! - [Leds] is the state of N LEDs under one indication: the levels to show this tick, HAL-free.
//! - [Channel] is one PWM output; [Pwm] adapts any embedded-hal [SetDutyCycle](embedded_hal::pwm::SetDutyCycle)
//!   (active-low option).
//! - `Indication` (feature `embassy`) is the shared state in a `static` and the task loop; the board's closure
//!   writes each tick's levels to its channels.
//! - `indication` (feature `ww-indication`) is the bookkeeping behind a `ww_indication::Indication` server.
//!
//! All per-tick math is u32, without 64-bit libcalls: the Cortex-M0+ has no 64-bit multiply and no hardware divide.
//! Only aligning to shared time after a mode change or a clock step takes one 64-bit remainder.

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

/// Shared time for [Animation::align]: µs since the Unix epoch (UTC), and the shared clock's generation, which
/// changes when shared time steps or switches source (bedrock_sync's `SharedNow`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub struct SharedTime {
    pub us: u64,
    pub generation: u32,
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
    /// Aligned to shared time ([align](Self::align)): the low 32 bits of the shared µs the current cycle started at,
    /// valid for the clock generation `anchor_gen`; `None` until aligned, and after a mode change.
    anchor: u32,
    anchor_gen: Option<u32>,
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
            anchor: 0,
            anchor_gen: None,
        }
    }

    /// Period in µs of a breath or blink that can be aligned (up to 71 minutes), `None` otherwise. Never 0.
    fn period_us(&self) -> Option<u32> {
        let ms = match self.mode {
            Mode::Breath { period_ms } => period_ms.max(TICK_MS),
            // new() turns a blink without on or off time into Off or On: never 0
            Mode::Blink { on_ms, off_ms } => on_ms.checked_add(off_ms)?.max(1),
            _ => return None,
        };
        // a compare, not checked_mul: its overflow check is a 64-bit multiply (libcall) on the M0+
        (ms <= u32::MAX / 1000).then(|| ms * 1000)
    }

    /// Puts the animation where it is at shared time `t` when every cycle starts on a whole multiple of its period
    /// since the Unix epoch, WSPR-style: animations of the same mode on several devices are in phase, with nothing
    /// passed around (STD-50 in ww_indication). Call it every tick while there is shared time; [advance](Self::advance)
    /// goes on from here when it stops. Off, On and periods over 71 minutes are left as they are.
    ///
    /// Per tick: 32-bit math (a divide for a blink); the cycle start is found with a 64-bit remainder only when the
    /// mode or the clock's generation changed.
    pub fn align(&mut self, t: SharedTime) {
        let Some(period) = self.period_us() else {
            return;
        };
        if self.anchor_gen != Some(t.generation) {
            self.anchor_at(t, period);
        }
        let mut pos = (t.us as u32).wrapping_sub(self.anchor);
        if pos >= period {
            // the next cycle, usually; anything farther (a long pause between calls) starts over
            if pos - period < period {
                self.anchor = self.anchor.wrapping_add(period);
                pos -= period;
            } else {
                pos = self.anchor_at(t, period);
            }
        }
        self.phase = match self.mode {
            // pos < period: below 2^32
            Mode::Breath { .. } => pos * (u32::MAX / period.max(1)),
            _ => pos / 1000,
        };
    }

    /// Finds the start of the cycle `t` is in; returns how far into it `t` is (µs).
    #[cold]
    #[inline(never)]
    fn anchor_at(&mut self, t: SharedTime, period: u32) -> u32 {
        let pos = (t.us % period as u64) as u32;
        self.anchor = (t.us as u32).wrapping_sub(pos);
        self.anchor_gen = Some(t.generation);
        pos
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
                let fade = self.step.max(1); // >= 1 already (new()): no divide-by-zero panic path
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

/// Smoothstep 3x^2 - 2x^3 on Q16 (0..=65535), for soft starts and stops: floor(x^2 (3 - 2x)), exact to the last
/// count and monotonic, in 32-bit multiplies only (no 64-bit libcall on the Cortex-M0+).
pub fn ease(x: u32) -> u32 {
    let x = x.min(FULL);
    let a = x * x; // < 2^32
    let c = 3 * 32768 - x; // (3 - 2x) / 2 in Q16: 32769..=98304
    let (ah, al) = (a >> 16, a & 0xffff);
    let (ch, cl) = (c >> 16, c & 0xffff);
    // a * c / 2^31 as (ah * c + al * ch) * 2^16 + al * cl, every part below 2^32: ah * c grows with x up to
    // 65534 * 32769 < 2^31 at x = 65535; flooring by 2^16 and then by 2^15 is the same as by 2^31
    let h = ah * c + al * ch;
    (h + ((al * cl) >> 16)) >> 15
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

    /// The same curve in 64 bits, the reference.
    fn ease_u64(x: u32) -> u32 {
        let x = x.min(FULL) as u64;
        ((x * x * (3 * 65536 - 2 * x)) >> 32) as u32
    }

    /// Donguru's ease before bedrock_led: 32-bit, the square rounded down first.
    fn ease_donguru(x: u32) -> u32 {
        let x2 = (x * x) >> 16;
        (x2 * ((3 * 65536 - 2 * x) >> 2)) >> 14
    }

    #[test]
    fn ease_is_exact() {
        for x in 0..=70000 {
            assert_eq!(ease(x), ease_u64(x), "ease({x})");
        }
        // within 3 counts of the old Donguru curve
        let worst = (0..=65535)
            .map(|x| ease(x).abs_diff(ease_donguru(x)))
            .max()
            .unwrap();
        assert_eq!(worst, 3);
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

    const UTC: u64 = 1_761_000_000_000_000; // a multiple of 3 s and 1 s

    fn at(us: u64) -> SharedTime {
        SharedTime { us, generation: 1 }
    }

    #[test]
    fn aligned_breath_starts_on_period_boundaries() {
        let mut a = Animation::new(Mode::Breath { period_ms: 3000 });
        a.align(at(UTC));
        assert_eq!(a.level(), 0, "dark at a multiple of the period");
        a.align(at(UTC + 1_500_000));
        assert!(a.level() > 65000, "top half way");
        a.align(at(UTC + 3_000_000));
        assert_eq!(a.level(), 0);
        a.align(at(UTC + 3_750_000));
        let quarter = a.level();
        assert!(quarter > 20000 && quarter < 45000, "{quarter}");
    }

    #[test]
    fn aligned_devices_agree_whenever_their_mode_was_set() {
        let mode = Mode::Breath { period_ms: 2000 };
        let mut early = Animation::new(mode);
        for _ in 0..137 {
            early.advance();
        }
        let mut late = Animation::new(mode);
        for i in 0..500u64 {
            let t = at(UTC + 777 + i * 10_000);
            early.align(t);
            late.align(t);
            assert_eq!(early.level(), late.level(), "tick {i}");
            early.advance();
            late.advance();
        }
    }

    #[test]
    fn aligned_blink_is_on_at_the_start_of_each_cycle() {
        let mut a = Animation::new(Mode::Blink {
            on_ms: 500,
            off_ms: 500,
        });
        let level = |a: &mut Animation, us| {
            a.align(at(us));
            a.level()
        };
        assert_eq!(level(&mut a, UTC), 0);
        assert!(level(&mut a, UTC + 200_000) >= 65533);
        assert_eq!(level(&mut a, UTC + 700_000), 0);
        assert!(level(&mut a, UTC + 1_200_000) >= 65533);
        // a long pause between calls: found again
        assert!(level(&mut a, UTC + 3_600_000_200_000) >= 65533);
    }

    #[test]
    fn free_running_takes_over_where_aligned_left_off() {
        let mut a = Animation::new(Mode::Breath { period_ms: 1000 });
        a.align(at(UTC + 250_000));
        let before = a.level();
        a.advance();
        let after = a.level();
        // one tick on from the aligned phase, no jump back to dark
        assert!(
            after > before && after - before < 4000,
            "{before} -> {after}"
        );
    }

    #[test]
    fn a_new_generation_finds_the_cycle_again() {
        let mut a = Animation::new(Mode::Breath { period_ms: 1000 });
        a.align(at(UTC + 100_000));
        // the clock stepped by 0.4 s
        a.align(SharedTime {
            us: UTC + 500_000,
            generation: 2,
        });
        let mut b = Animation::new(Mode::Breath { period_ms: 1000 });
        b.align(at(UTC + 500_000));
        assert_eq!(a.level(), b.level());
    }

    #[test]
    fn off_on_and_long_periods_stay_as_they_are() {
        for mode in [Mode::Off, Mode::On] {
            let mut a = Animation::new(mode);
            let l = a.level();
            a.align(at(UTC + 5));
            assert_eq!(a.level(), l);
        }
        // 2 hours: not aligned, runs free from dark
        let mut a = Animation::new(Mode::Breath {
            period_ms: 7_200_000,
        });
        a.align(at(UTC + 3_000_000_000));
        assert_eq!(a.level(), 0);
    }
}
