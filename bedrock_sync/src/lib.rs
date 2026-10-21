#![cfg_attr(not(test), no_std)]
//! Shared clock for bedrock firmware (DEV-13): the device's free-running local clock, disciplined onto UTC by the
//! steer points of its host (`ww_timebase::Clock::steer`, the host side is wire_weaver's `ww_clock_sync`) or by a
//! sync line, so several boards breathe their LEDs in phase and can do something at the same shared time.
//!
//! - [SharedClock] is the clock: HAL-free, integer math, host-tested. It follows its best source (a locked sync
//!   line before the host), slews to a new point instead of stepping, and coasts in holdover when the points stop.
//! - [Oscillator] is an optional backend for chips that can trim their own clock (a fractional PLL): the rate is
//!   then corrected in hardware and the software correction stays near zero.
//! - `Sync` (feature `embassy`) is a [SharedClock] in a `static` on embassy-time's `Instant`, with
//!   `wait_until_shared` for scheduled actions.
//! - `clock` (feature `ww-timebase`) is the bookkeeping behind a `ww_timebase::Clock` server.
//!
//! Local and shared time are µs as `u64`. The hot path ([SharedClock::shared], every LED tick) adds and compares
//! 64-bit values but multiplies only in 32 bits, so the Cortex-M0+ (no 64-bit multiply, no divide) calls no
//! 64-bit libcall there; steer points and long gaps take a cold path with 64-bit math, once a second at most.

#[cfg(feature = "ww-timebase")]
pub mod clock;
#[cfg(feature = "embassy")]
mod driver;
mod osc;

#[cfg(feature = "embassy")]
pub use driver::{NotSynced, SharedNow, Sync};
pub use osc::{NoOscillator, Oscillator};

/// Fraction bits of the rate: 2^-20 is about 0.95 ppm.
const Q: u32 = 20;
/// Longest stretch of local time the hot path advances in one go (32.8 ms): its 32-bit products can't overflow.
const CHUNK: u64 = 1 << 15;
/// Largest rate correction followed, in Q20: ±3 % (an RC oscillator is ±1-2 % off).
const RATE_MAX: i32 = 31_457;

/// Where the shared clock takes its time from, best first.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum Source {
    /// A sync line this device is a slave on (IRIG-B, PPS): µs between the boards on the line.
    Line,
    /// The host steering it over its API: tens of µs to the host, and the host's UTC is as good as its NTP.
    Host,
}

/// State of the shared clock.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum State {
    /// Never steered, or coasting for longer than [Config::free_after_us]: [SharedClock::shared] still runs (on the
    /// last line, once there was one) but nothing vouches for it.
    Free,
    /// Steer points come in: the shared clock follows [SharedClock::source].
    Locked,
    /// Steer points stopped coming: coasting on the last offset and rate.
    Holdover,
}

/// A steer point, as `ww_timebase::SteerPoint` has it with `local` unwrapped: the device's local time `local_us`
/// is shared time `shared_us`, and shared time runs `rate_ppb` parts per billion faster than local time.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub struct Steer {
    pub local_us: u64,
    pub shared_us: u64,
    pub rate_ppb: i32,
    /// How far `shared_us` may be off, as the source estimates it.
    pub error_us: u32,
}

/// What a steer point did, see [SharedClock::steer].
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum Steered {
    /// Followed, slewing towards it by this many µs (0: already on it).
    Slewing(i64),
    /// Followed by a step: the first point, or one farther off than [Config::step_above_us].
    Stepped,
    /// Only noted: the clock follows a better source (a locked line, when the host steers).
    Ignored(Source),
}

/// Why a steer point was refused.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum SteerError {
    /// The rate is beyond ±3 %: a clock that far off is broken, or the host's fit is.
    RateOutOfRange,
}

/// Holdover, slewing and steps.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub struct Config {
    /// Without a steer point for this long, the clock is in holdover (3 s: three missed points of a host).
    pub holdover_after_us: u64,
    /// In holdover for this long, the clock counts as free again (1 h).
    pub free_after_us: u64,
    /// How fast an offset is slewed away, in ppm of local time (10 000: 1 %, 10 ms in a second).
    pub slew_ppm: u32,
    /// A new point farther off than this is stepped to instead of slewed (1 s: something went wrong).
    pub step_above_us: u64,
    /// How fast the error grows in holdover, in ppb (10 ppm: an RC oscillator's temperature drift).
    pub holdover_drift_ppb: u32,
}

impl Config {
    pub const DEFAULT: Config = Config {
        holdover_after_us: 3_000_000,
        free_after_us: 3_600_000_000,
        slew_ppm: 10_000,
        step_above_us: 1_000_000,
        holdover_drift_ppb: 10_000,
    };
}

impl Default for Config {
    fn default() -> Self {
        Self::DEFAULT
    }
}

/// The shared clock: local time (µs) to shared time (µs since the Unix epoch, UTC) along the last steer point's
/// line, slewing out the difference to each new one. See the [crate docs](crate).
#[derive(Clone, Debug)]
pub struct SharedClock {
    config: Config,
    /// The line, as of local time `base_local`: shared time `base_shared`, `frac` / 2^20 µs of rate correction
    /// carried.
    base_local: u64,
    base_shared: u64,
    frac: i32,
    /// Shared minus local rate, Q20.
    rate: i32,
    /// Offset still to slew in, µs, and its fraction carried.
    slew_left: i64,
    slew_frac: i32,
    /// Slew rate, Q20.
    slew: i32,
    /// Ever steered: `base_*` hold a line.
    steered: bool,
    /// Local time of the last point followed (or noted from the followed source).
    last_point: u64,
    source: Option<Source>,
    /// The followed source's error at its last point.
    error_us: u32,
    /// The host's rate of the last point, the oscillator trim asked for (unquantised, so steps smaller than the
    /// hardware's add up) and the trim applied, ppb.
    rate_ppb: i32,
    trim_target: i32,
    trim_ppb: i32,
    generation: u32,
}

impl SharedClock {
    pub const fn new(config: Config) -> Self {
        SharedClock {
            config,
            base_local: 0,
            base_shared: 0,
            frac: 0,
            rate: 0,
            slew_left: 0,
            slew_frac: 0,
            slew: ((config.slew_ppm as u64 * (1 << Q)) / 1_000_000) as i32,
            steered: false,
            last_point: 0,
            source: None,
            error_us: 0,
            rate_ppb: 0,
            trim_target: 0,
            trim_ppb: 0,
            generation: 0,
        }
    }

    /// Shared time at local time `local_us` (not before the last call's), or `None` before the first steer point.
    /// The hot path: called every LED tick, 64-bit adds only for gaps up to 32 ms.
    #[inline]
    pub fn shared(&mut self, local_us: u64) -> Option<u64> {
        if !self.steered {
            return None;
        }
        let d = local_us.saturating_sub(self.base_local);
        if d < CHUNK {
            self.advance_chunk(d as i32);
        } else {
            self.advance_far(d);
        }
        Some(self.base_shared)
    }

    /// Advances the line by `d` < [CHUNK] µs of local time: rate correction and slew, in 32-bit products.
    #[inline(always)]
    fn advance_chunk(&mut self, d: i32) {
        // d < 2^15, |rate| <= 2^15: below 2^30
        let total = d * self.rate + self.frac;
        let corr = total >> Q;
        self.frac = total - (corr << Q);
        let mut shared = self.base_shared.wrapping_add((d + corr) as i64 as u64);
        if self.slew_left != 0 {
            let total = d * self.slew + self.slew_frac;
            let mut step = (total >> Q) as i64;
            self.slew_frac = total - ((step as i32) << Q);
            if step >= self.slew_left.abs() {
                step = self.slew_left.abs();
                self.slew_frac = 0;
            }
            let step = if self.slew_left < 0 { -step } else { step };
            self.slew_left -= step;
            shared = shared.wrapping_add(step as u64);
        }
        self.base_shared = shared;
        self.base_local += d as u64;
    }

    /// Advances over a long gap: whole chunks are cheap but would loop for hours after a long sleep, so the bulk
    /// goes in 64-bit math. Cold: after a gap of more than 32 ms only.
    #[cold]
    #[inline(never)]
    fn advance_far(&mut self, d: u64) {
        let bulk = d - d % CHUNK;
        let total = bulk as i64 * self.rate as i64 + self.frac as i64;
        let corr = total >> Q;
        self.frac = (total - (corr << Q)) as i32;
        let mut shared = self.base_shared.wrapping_add((bulk as i64 + corr) as u64);
        if self.slew_left != 0 {
            let room = ((bulk as i64 * self.slew as i64) >> Q).min(self.slew_left.abs());
            let step = if self.slew_left < 0 { -room } else { room };
            self.slew_left -= step;
            shared = shared.wrapping_add(step as u64);
        }
        self.base_shared = shared;
        self.base_local += bulk;
        self.advance_chunk((d - bulk) as i32);
    }

    /// Takes a steer point from `source` at local time `now_us`: followed unless a better source is locked, slewed
    /// to unless it is the first one or farther off than [Config::step_above_us]. With an [Oscillator] backend the
    /// rate goes to the oscillator first ([steer_with](Self::steer_with)).
    pub fn steer(
        &mut self,
        now_us: u64,
        point: Steer,
        source: Source,
    ) -> Result<Steered, SteerError> {
        self.steer_with(now_us, point, source, &mut NoOscillator)
    }

    /// [steer](Self::steer), trimming `osc` by part of the point's rate, so the local clock itself runs at the
    /// shared rate after a few points and the software correction keeps only what the oscillator can't do.
    pub fn steer_with(
        &mut self,
        now_us: u64,
        point: Steer,
        source: Source,
        osc: &mut impl Oscillator,
    ) -> Result<Steered, SteerError> {
        let state = self.state(now_us);
        if source == Source::Host && self.source == Some(Source::Line) && state == State::Locked {
            return Ok(Steered::Ignored(Source::Line));
        }
        // the oscillator takes a share of the rate; the software keeps the rest
        let trim_step = osc.trim_step_ppb(point.rate_ppb);
        let mut rate_ppb = point.rate_ppb;
        if trim_step != 0 {
            self.trim_target = self.trim_target.saturating_add(trim_step);
            let applied = osc.set_trim_ppb(self.trim_target);
            // more than 1 ppm short of the target: the hardware clamped it, don't wind up beyond its range
            if (self.trim_target - applied).abs() > 1_000 {
                self.trim_target = applied;
            }
            // trimming the oscillator faster by x ppb makes the local clock run x faster: shared runs x slower
            // relative to it
            rate_ppb = rate_ppb.saturating_sub(applied - self.trim_ppb);
            self.trim_ppb = applied;
        }
        let rate = q20(rate_ppb).ok_or(SteerError::RateOutOfRange)?;
        // where the point puts shared time now
        let target = at(point.local_us, point.shared_us, rate, now_us);
        let steered = match self.shared(now_us) {
            Some(current) if target.abs_diff(current) <= self.config.step_above_us => {
                let off = target as i64 - current as i64;
                self.slew_left = off;
                self.slew_frac = 0;
                Steered::Slewing(off)
            }
            _ => {
                self.step(now_us, target);
                Steered::Stepped
            }
        };
        if self.source != Some(source) {
            self.generation = self.generation.wrapping_add(1);
        }
        self.rate = rate;
        self.rate_ppb = point.rate_ppb;
        self.source = Some(source);
        self.last_point = now_us;
        self.error_us = point.error_us;
        Ok(steered)
    }

    fn step(&mut self, now_us: u64, shared_us: u64) {
        self.base_local = now_us;
        self.base_shared = shared_us;
        self.frac = 0;
        self.slew_left = 0;
        self.slew_frac = 0;
        self.steered = true;
        self.generation = self.generation.wrapping_add(1);
    }

    /// State at local time `now_us`.
    pub fn state(&self, now_us: u64) -> State {
        if !self.steered {
            return State::Free;
        }
        let since = now_us.saturating_sub(self.last_point);
        if since <= self.config.holdover_after_us {
            State::Locked
        } else if since <= self.config.holdover_after_us + self.config.free_after_us {
            State::Holdover
        } else {
            State::Free
        }
    }

    /// The source followed (`Locked`) or last followed (`Holdover`, `Free` after a lock).
    pub fn source(&self) -> Option<Source> {
        self.source
    }

    /// Estimated error of shared time at `now_us`, µs: the source's own at its last point, plus the holdover drift
    /// since, plus what is still to slew. `None` before the first point.
    pub fn error_us(&self, now_us: u64) -> Option<u32> {
        if !self.steered {
            return None;
        }
        let coasting = now_us.saturating_sub(self.last_point + self.config.holdover_after_us);
        let drift = coasting / 1_000 * self.config.holdover_drift_ppb as u64 / 1_000_000;
        Some(
            (self.error_us as u64 + drift + self.slew_left.unsigned_abs()).min(u32::MAX as u64)
                as u32,
        )
    }

    /// Changes at every step and source switch: whatever a reader derived from shared time (an LED cycle's start)
    /// is stale. Slews keep it, since shared time stays continuous.
    pub fn generation(&self) -> u32 {
        self.generation
    }

    /// Local time at which shared time reaches `shared_us` along the current line (the slew left counted in), or
    /// `None` before the first point. For a scheduled action: wake then, and check [shared](Self::shared) again
    /// (a steer point in between moves it).
    pub fn local_at(&self, shared_us: u64) -> Option<u64> {
        if !self.steered {
            return None;
        }
        let ahead = shared_us as i64 - self.base_shared as i64 - self.slew_left;
        // local = ahead / (1 + rate): the fixed point of local = ahead - local * rate, the error shrinks by the
        // rate (at most 3 %) per round
        let mut local = ahead;
        for _ in 0..4 {
            local = ahead - ((local * self.rate as i64) >> Q);
        }
        Some((self.base_local as i64 + local).max(self.base_local as i64) as u64)
    }

    /// The last steer point's rate (ppb, as the source sent it) and the oscillator trim applied (ppb).
    pub fn rates(&self) -> (i32, i32) {
        (self.rate_ppb, self.trim_ppb)
    }

    /// Local time of the last point taken from the followed source.
    pub fn last_point(&self) -> Option<u64> {
        self.steered.then_some(self.last_point)
    }
}

impl Default for SharedClock {
    fn default() -> Self {
        Self::new(Config::DEFAULT)
    }
}

/// `ppb` in Q20 (rounded), `None` beyond [RATE_MAX]. Cold: once per steer point (a 64-bit division).
fn q20(ppb: i32) -> Option<i32> {
    let q =
        (ppb as i64 * (1 << Q) + if ppb < 0 { -500_000_000 } else { 500_000_000 }) / 1_000_000_000;
    (q.abs() <= RATE_MAX as i64).then_some(q as i32)
}

/// Shared time at local `now` along the line through (`local`, `shared`) with `rate` (Q20). Cold (a 64-bit
/// multiply), `now - local` below 2^47 µs.
fn at(local: u64, shared: u64, rate: i32, now: u64) -> u64 {
    let d = now as i64 - local as i64;
    let corr = (d * rate as i64) >> Q;
    (shared as i64 + d + corr) as u64
}

#[cfg(test)]
mod tests;
