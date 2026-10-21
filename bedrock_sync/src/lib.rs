#![cfg_attr(not(test), no_std)]
//! Shared clock for bedrock firmware (DEV-13): the device's free-running local clock, disciplined onto UTC by the
//! steer points of its host (`ww_timebase::Clock::steer`, the host side is wire_weaver's `ww_clock_sync`) or by a
//! sync line, so several boards breathe their LEDs in phase and can do something at the same shared time.
//!
//! - [SharedClock] is the clock: HAL-free, integer math, host-tested. It follows its best source (a locked sync
//!   line before the host), slews to a new point instead of stepping, and coasts in holdover when the points stop.
//!   [State] says how far to trust it: `Acquiring` while it follows a source but is farther off than
//!   [Config::lock_within_us] (slewing fast, 5 %), `Locked` once within (slewing at 1 %).
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
/// Fastest slew, in Q20: 6 %. [CHUNK] times it plus a carried fraction stays below 2^31.
const SLEW_MAX: i32 = 62_914;

/// Where the shared clock takes its time from. By default ([Sources] not set) a locked line before the host.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum Source {
    /// A sync line this device is a slave on: µs between the boards on the line.
    Line(Line),
    /// The host steering it over its API: tens of µs to the host, and the host's UTC is as good as its NTP.
    Host,
}

/// The format of a sync line, as `ww_timebase::SyncFormat`.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum Line {
    IrigB,
    Pps,
}

/// An explicit source list, best first, see [SharedClock::set_sources]: `[Line(IrigB), Host]`, or `[Line(IrigB)]`
/// for "never let a PC steer me". Up to [Sources::MAX] sources, each once.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub struct Sources {
    list: [Source; Sources::MAX],
    len: u8,
}

/// Why a source list was refused.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum SourcesError {
    /// More than [Sources::MAX] sources.
    TooMany,
    /// A source named twice.
    Twice,
}

impl Sources {
    /// Most sources in a list: every line format and the host.
    pub const MAX: usize = 4;

    pub fn new(sources: &[Source]) -> Result<Self, SourcesError> {
        let mut list = [Source::Host; Self::MAX];
        if sources.len() > Self::MAX {
            return Err(SourcesError::TooMany);
        }
        for (i, s) in sources.iter().enumerate() {
            if sources[..i].contains(s) {
                return Err(SourcesError::Twice);
            }
            list[i] = *s;
        }
        Ok(Sources {
            list,
            len: sources.len() as u8,
        })
    }

    pub fn as_slice(&self) -> &[Source] {
        &self.list[..self.len as usize]
    }
}

/// State of the shared clock.
///
/// `Acquiring` and `Locked` both follow a source; what tells them apart is how close the clock is: the offset
/// still to slew in plus the source's own error, against [Config::lock_within_us]. A clock enters `Locked` once
/// that is within the setting and leaves it again (back to `Acquiring`) when a steer point finds it more than twice
/// the setting off, or steps it.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum State {
    /// Never steered, or coasting for longer than [Config::free_after_us]: [SharedClock::shared] still runs (on the
    /// last line, once there was one) but nothing vouches for it.
    Free,
    /// Steer points come in and the clock follows [SharedClock::source], but it isn't within
    /// [Config::lock_within_us] yet: it slews at [Config::acquire_slew_ppm].
    Acquiring,
    /// Steer points come in and the clock is within [Config::lock_within_us] of [SharedClock::source]: what it
    /// says is shared time to that, and LEDs on several boards are in phase. Slews at [Config::slew_ppm].
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
    /// Only noted: the source isn't allowed by the list, or the clock follows a better one that is locked. The source
    /// followed, `None` for none.
    Ignored(Option<Source>),
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
    /// How fast an offset is slewed away once locked, in ppm of local time (10 000: 1 %, 10 ms in a second).
    pub slew_ppm: u32,
    /// How fast an offset is slewed away while acquiring (50 000: 5 %, 250 ms in 5 s). Up to 6 %; shared time
    /// still never jumps or runs backwards.
    pub acquire_slew_ppm: u32,
    /// The clock is `Locked` once the offset still to slew plus the source's error is within this, µs; `Acquiring`
    /// until then. 2 ms: a full-speed USB board's own error bound is about 1 ms (half its shortest round trip and
    /// the scatter of the host's fit, Donguru), and LEDs look in phase far beyond that.
    pub lock_within_us: u32,
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
        acquire_slew_ppm: 50_000,
        lock_within_us: 2_000,
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
    /// Was within [Config::lock_within_us] at or since the last point: `Locked` until a point finds it twice that
    /// far off.
    locked: bool,
    /// Ever steered: `base_*` hold a line.
    steered: bool,
    /// Local time of the last point followed (or noted from the followed source).
    last_point: u64,
    source: Option<Source>,
    /// The explicit source list, `None`: a locked line before the host.
    sources: Option<Sources>,
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
            slew: slew_q20(config.acquire_slew_ppm),
            locked: false,
            steered: false,
            last_point: 0,
            source: None,
            sources: None,
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
        let was = self.state(now_us);
        let followed = self
            .source
            .filter(|_| matches!(was, State::Locked | State::Acquiring));
        let Some(rank) = self.rank(source) else {
            return Ok(Steered::Ignored(followed));
        };
        if let Some(f) = followed
            && f != source
            && self.rank(f).is_some_and(|r| r < rank)
        {
            return Ok(Steered::Ignored(Some(f)));
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
        // how far off the point found the clock: in lock within the setting, out of it beyond twice that
        let within = self.config.lock_within_us as u64;
        let off = match steered {
            Steered::Slewing(off) => off.unsigned_abs() + point.error_us as u64,
            _ => u64::MAX,
        };
        self.locked = self.source == Some(source)
            && if was == State::Locked {
                off <= 2 * within
            } else {
                off <= within
            };
        self.slew = slew_q20(if self.locked {
            self.config.slew_ppm
        } else {
            self.config.acquire_slew_ppm
        });
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

    /// Replaces the fixed ranking (a locked line before the host) with an explicit list, best first; `None` goes
    /// back to the fixed ranking. A source the list leaves out is only noted from then on: if it is the one followed,
    /// the clock goes into holdover on it until an allowed source steers it (slewed, as any switch).
    pub fn set_sources(&mut self, sources: Option<Sources>) {
        self.sources = sources;
    }

    /// The explicit source list, `None` for the fixed ranking.
    pub fn sources(&self) -> Option<&[Source]> {
        self.sources.as_ref().map(Sources::as_slice)
    }

    /// Place of `source` in the ranking, best first; `None` when it isn't allowed.
    fn rank(&self, source: Source) -> Option<usize> {
        match &self.sources {
            Some(list) => list.as_slice().iter().position(|s| *s == source),
            None => Some(match source {
                Source::Line(_) => 0,
                Source::Host => 1,
            }),
        }
    }

    /// State at local time `now_us`.
    pub fn state(&self, now_us: u64) -> State {
        if !self.steered {
            return State::Free;
        }
        let since = now_us.saturating_sub(self.last_point);
        let allowed = self.source.is_some_and(|s| self.rank(s).is_some());
        if since <= self.config.holdover_after_us && allowed {
            let off = self.slew_left_at(now_us).unsigned_abs() + self.error_us as u64;
            if self.locked || off <= self.config.lock_within_us as u64 {
                State::Locked
            } else {
                State::Acquiring
            }
        } else if since <= self.config.holdover_after_us + self.config.free_after_us {
            State::Holdover
        } else {
            State::Free
        }
    }

    /// Offset still to slew in at local time `now_us` (not before the last [shared](Self::shared) call), µs:
    /// positive while shared time is being sped up to catch up. 0 before the first point.
    pub fn slew_left_at(&self, now_us: u64) -> i64 {
        let d = now_us.saturating_sub(self.base_local);
        let done =
            (d.saturating_mul(self.slew as u64) >> Q).min(self.slew_left.unsigned_abs()) as i64;
        self.slew_left - self.slew_left.signum() * done
    }

    /// The source followed (`Acquiring`, `Locked`) or last followed (`Holdover`, `Free` after a lock).
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
            (self.error_us as u64 + drift + self.slew_left_at(now_us).unsigned_abs())
                .min(u32::MAX as u64) as u32,
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

/// A slew of `ppm` in Q20, at most [SLEW_MAX]. Cold: once per steer point.
const fn slew_q20(ppm: u32) -> i32 {
    let q = (ppm as u64 * (1 << Q)) / 1_000_000;
    if q > SLEW_MAX as u64 {
        SLEW_MAX
    } else {
        q as i32
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
