//! USB frames as a time reference (feature `sof`, USBD-10): the device's local clock measured against the start
//! of frame (SOF) packets of its host controller, which come every millisecond with an 11-bit frame number.
//!
//! The USB driver timestamps each SOF in its interrupt handler with the local clock (bedrock_usb's
//! `state!(static USB_STATE: Desc, sof = now_us)`); the firmware hands the last one to a [SofTrack] now and then,
//! from any task (every few ms to every second). The track
//!
//! - unwraps the 11-bit frame number into a frame count, using the local time between two samples,
//! - keeps one point per window of [WINDOW] frames, the sample with the least interrupt latency (a timestamp is the
//!   SOF plus a latency that is never negative, so the earliest one is the closest),
//! - fits the local clock's rate against the frames over a baseline of up to [MAX_BASELINE] frames, and
//! - says for any frame number near now when that SOF came, in local µs ([SofTrack::local_at]).
//!
//! ## What it means for the host
//!
//! Every device on one host controller (one bus: the root hub and the hubs below it) sees the same SOFs, so frame
//! numbers are a time scale they share, to within each one's interrupt latency and the constant delay of its path
//! through the hubs. The host can't timestamp a SOF with its own clock through the usual user-space USB APIs, so it
//! uses frames in two ways:
//!
//! - **Frame-referenced steer points.** The host's estimator (wire_weaver's `ww_clock_sync`) learns the mapping
//!   from frames to its shared time per host controller, from the round trips of all devices on it together (their
//!   [FrameMap]s tie each round trip to a frame) and over a long baseline, so the round trips' jitter averages
//!   out. It then steers a device with "frame F began at shared time S" ([FrameSteer]); the device turns that into
//!   an ordinary [Steer] at its local time of frame F ([SofTrack::steer]). The jitter of the bulk transfer that
//!   carried the point no longer reaches the device's clock: devices on one controller agree to within their
//!   interrupt latency, even when the host's own UTC is only as good as its NTP.
//! - **Rate.** [SofTrack::rate_ppb] is the local clock against the host controller's frame clock, which runs from
//!   the controller's crystal. The host compares that with the rate it fits from round trips to tell its
//!   controller's error from the device's.
//!
//! A device on another host controller has its own frame numbers: frame-referenced points are only ever made for
//! the controller the device hangs on, and a device that moves sees its frames jump ([Push::Restarted]).
//!
//! The track never steers on its own: the firmware decides which points reach its [crate::SharedClock].
//!
//! The hot path ([SofTrack::push] within a window) compares and adds 64-bit values; closing a window, a rate fit
//! and [SofTrack::steer] divide in 64 bits (a libcall on the Cortex-M0+), once per window at most.

use crate::Steer;

/// Frames per point: the track keeps the earliest sample of each window of 128 frames (128 ms).
pub const WINDOW: i64 = 128;

/// Longest baseline of the rate fit, in frames (65.5 s): long enough that a few µs of latency are well below a ppm,
/// short enough to follow a crystal's drift with temperature. The oldest point moves on in halves of it.
pub const MAX_BASELINE: i64 = 1 << 16;

/// Shortest baseline the track gives a rate from (1.02 s): 10 µs of latency at each end are then 20 ppm.
pub const MIN_BASELINE: i64 = 8 * WINDOW;

/// Longest gap between two samples the frame count survives (8 s): at the ±3 % an RC oscillator may be off, the
/// local time then still names the frame count within 240 frames, well inside the 1024 an 11-bit number allows.
pub const MAX_GAP_US: u64 = 8_000_000;

/// A sample later than the fitted line by more than this (µs) is not taken as a point: an interrupt held off for
/// half a frame.
pub const LATE_US: i64 = 500;

/// A sample earlier than the fitted line by more than this (µs) can't be latency: the frames jumped (another host,
/// a bus that restarted its frame count), and the track starts over.
pub const EARLY_US: i64 = 200;

/// Consecutive late samples after which the track starts over: the line itself is wrong.
pub const LATE_RUN: u8 = 8;

/// Nominal frame length at full speed, µs.
const FRAME_US: i64 = 1000;
/// Largest rate followed, ppb: ±3 %, as [crate::SharedClock].
const RATE_MAX_PPB: i64 = 30_000_000;

/// How far the track is.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum SofState {
    /// No sample yet, or started over.
    Idle,
    /// Counting frames, the baseline still shorter than [MIN_BASELINE]: no rate, no frame times.
    Acquiring,
    /// A rate fitted: [SofTrack::local_at], [SofTrack::map] and [SofTrack::steer] answer.
    Tracking,
}

/// Why the track started over.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum Restart {
    /// More than [MAX_GAP_US] since the last sample (suspend, a USB task that didn't run).
    Gap,
    /// The local time went backwards.
    Backwards,
    /// The frames no longer fit the line: earlier than [EARLY_US] or [LATE_RUN] samples late in a row.
    Jump,
}

/// What a sample did, see [SofTrack::push].
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum Push {
    /// The first sample of a run.
    Started,
    /// The same SOF as the last sample: nothing to do.
    Same,
    /// Counted, and a candidate for its window's point.
    Taken,
    /// Counted, but too late after its SOF to be a point.
    Late,
    /// The track started over with this sample as its first.
    Restarted(Restart),
}

/// The local time of a recent frame and the rate against frames: what a device tells its host so the host can tie
/// its round trips to frames (see the module docs).
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub struct FrameMap {
    /// The 11-bit frame number.
    pub frame: u16,
    /// Local µs at that SOF, the earliest sample of its window.
    pub local_us: u64,
    /// The local clock runs this many parts per billion faster than the frame clock: a frame lasts
    /// `1000 * (1 + rate_ppb / 1e9)` local µs.
    pub rate_ppb: i32,
    /// Frames the rate was fitted over.
    pub baseline_frames: u32,
}

/// A steer point referenced to a frame: frame `frame` began at shared time `shared_us`, and shared time runs
/// `rate_ppb` faster than the frame clock. [SofTrack::steer] makes it a [Steer].
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub struct FrameSteer {
    pub frame: u16,
    pub shared_us: u64,
    pub rate_ppb: i32,
    pub error_us: u32,
    pub steerer: u64,
}

/// One sample: frame count `n` since the run began, local time `t`.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
struct Sample {
    n: i64,
    t: u64,
    frame: u16,
}

/// The local clock against USB frames, see the module docs.
#[derive(Clone, Debug)]
pub struct SofTrack {
    last: Option<Sample>,
    /// The earliest sample of the window `last` is in, and that window's index.
    best: Option<Sample>,
    window: i64,
    /// The oldest point of the fit, the one it moves on to, and the newest.
    start: Option<Sample>,
    mid: Option<Sample>,
    end: Option<Sample>,
    rate_ppb: i32,
    tracking: bool,
    late: u8,
    restarts: u32,
}

impl Default for SofTrack {
    fn default() -> Self {
        Self::new()
    }
}

impl SofTrack {
    pub const fn new() -> Self {
        SofTrack {
            last: None,
            best: None,
            window: 0,
            start: None,
            mid: None,
            end: None,
            rate_ppb: 0,
            tracking: false,
            late: 0,
            restarts: 0,
        }
    }

    pub fn state(&self) -> SofState {
        match (self.last, self.tracking) {
            (None, _) => SofState::Idle,
            (Some(_), false) => SofState::Acquiring,
            (Some(_), true) => SofState::Tracking,
        }
    }

    /// Times the track started over.
    pub fn restarts(&self) -> u32 {
        self.restarts
    }

    /// A SOF with 11-bit frame number `frame` timestamped at local `local_us` (bedrock_usb's `Sof::frame` and
    /// `Sof::local_us`). The same SOF again is [Push::Same].
    pub fn push(&mut self, frame: u16, local_us: u64) -> Push {
        let frame = frame & 0x7ff;
        let Some(last) = self.last else {
            self.begin(frame, local_us);
            return Push::Started;
        };
        if local_us == last.t || frame == last.frame && local_us.abs_diff(last.t) < 1024 {
            return Push::Same;
        }
        if local_us < last.t {
            return self.restart(frame, local_us, Restart::Backwards);
        }
        let dt = local_us - last.t;
        if dt > MAX_GAP_US {
            return self.restart(frame, local_us, Restart::Gap);
        }
        // the frame count from the local time, its low 11 bits from the frame number
        let expect = (dt / FRAME_US as u64) as i64;
        let low = ((frame as i64) - (last.frame as i64)) & 0x7ff;
        let delta = low + ((expect - low + 1024).max(0) >> 11 << 11);
        let s = Sample {
            n: last.n + delta,
            t: local_us,
            frame,
        };
        self.last = Some(s);
        let mut late = false;
        if self.tracking {
            let r = self.residual(s);
            if r < -EARLY_US {
                return self.restart(frame, local_us, Restart::Jump);
            }
            if r > LATE_US {
                self.late += 1;
                if self.late >= LATE_RUN {
                    return self.restart(frame, local_us, Restart::Jump);
                }
                late = true;
            } else {
                self.late = 0;
            }
        }
        let window = s.n.div_euclid(WINDOW);
        if window != self.window {
            if let Some(b) = self.best.take() {
                self.point(b);
            }
            self.window = window;
        }
        if late {
            return Push::Late;
        }
        match self.best {
            Some(b) if self.residual(b) <= self.residual(s) => {}
            _ => self.best = Some(s),
        }
        Push::Taken
    }

    /// The local clock's rate against the frame clock, once [SofState::Tracking].
    pub fn rate_ppb(&self) -> Option<i32> {
        self.tracking.then_some(self.rate_ppb)
    }

    /// Local µs at the SOF of 11-bit frame number `frame`, the one nearest the last sample (within ±1024 frames),
    /// on the fitted line. `None` until [SofState::Tracking].
    pub fn local_at(&self, frame: u16) -> Option<u64> {
        let last = self.last?;
        if !self.tracking {
            return None;
        }
        let d = (((frame as i64) - (last.frame as i64) + 1024) & 0x7ff) - 1024;
        let n = last.n + d;
        u64::try_from(self.predict(n)).ok()
    }

    /// The newest point and the rate, once [SofState::Tracking].
    pub fn map(&self) -> Option<FrameMap> {
        let (end, start) = (self.end?, self.start?);
        self.tracking.then_some(FrameMap {
            frame: end.frame,
            local_us: end.t,
            rate_ppb: self.rate_ppb,
            baseline_frames: (end.n - start.n) as u32,
        })
    }

    /// A frame-referenced point as a [Steer] on the local clock: `local_us` the local time of the frame, the rate
    /// of shared time against local time from the host's rate against frames and this track's,
    /// `(1 + host) / (1 + local) - 1`. `None` until [SofState::Tracking], or for a frame more than 1024 frames from
    /// the last sample.
    pub fn steer(&self, p: FrameSteer) -> Option<Steer> {
        let local_us = self.local_at(p.frame)?;
        const B: i64 = 1_000_000_000;
        let local = self.rate_ppb as i64;
        let rate = (p.rate_ppb as i64 - local) * B / (B + local);
        Some(Steer {
            local_us,
            shared_us: p.shared_us,
            rate_ppb: rate.clamp(i32::MIN as i64, i32::MAX as i64) as i32,
            error_us: p.error_us,
            steerer: p.steerer,
        })
    }

    fn begin(&mut self, frame: u16, local_us: u64) {
        let s = Sample {
            n: 0,
            t: local_us,
            frame,
        };
        *self = SofTrack {
            last: Some(s),
            best: Some(s),
            restarts: self.restarts,
            ..SofTrack::new()
        };
    }

    fn restart(&mut self, frame: u16, local_us: u64, why: Restart) -> Push {
        self.restarts = self.restarts.wrapping_add(1);
        self.begin(frame, local_us);
        Push::Restarted(why)
    }

    /// A window's earliest sample becomes the newest point; the rate is fitted from the oldest.
    fn point(&mut self, p: Sample) {
        self.end = Some(p);
        let (Some(start), Some(mid)) = (self.start, self.mid) else {
            self.start = Some(p);
            self.mid = Some(p);
            return;
        };
        let start = if p.n - mid.n >= MAX_BASELINE / 2 {
            self.start = Some(mid);
            self.mid = Some(p);
            mid
        } else {
            start
        };
        let frames = p.n - start.n;
        if frames >= MIN_BASELINE {
            let off = (p.t - start.t) as i64 - frames * FRAME_US;
            self.rate_ppb = (off * 1_000_000 / frames).clamp(-RATE_MAX_PPB, RATE_MAX_PPB) as i32;
            self.tracking = true;
        }
    }

    /// Local µs of frame count `n` on the line through the newest point, or through the run's first sample before
    /// there is one.
    fn predict(&self, n: i64) -> i64 {
        let r = self.end.or(self.start).or(self.last).unwrap_or(Sample {
            n: 0,
            t: 0,
            frame: 0,
        });
        let d = n - r.n;
        r.t as i64 + d * FRAME_US + d * self.rate_ppb as i64 / 1_000_000
    }

    /// How much later than the line sample `s` is.
    fn residual(&self, s: Sample) -> i64 {
        s.t as i64 - self.predict(s.n)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A device whose local clock runs `rate` (fraction) fast against the frames, with its SOF interrupts late by a
    /// latency from a fixed pseudo-random sequence: mostly a few µs, now and then much more.
    struct Bench {
        rate: f64,
        t0: f64,
        seed: u32,
        frame0: u32,
    }

    impl Bench {
        fn new(rate_ppm: f64) -> Self {
            Bench {
                rate: rate_ppm * 1e-6,
                t0: 12_345_678.0,
                seed: 0x1234_5678,
                frame0: 1500,
            }
        }

        /// Local time of the SOF of frame `k` without latency.
        fn true_local(&self, k: u32) -> f64 {
            self.t0 + k as f64 * 1000.0 * (1.0 + self.rate)
        }

        fn latency(&mut self) -> u64 {
            // xorshift32
            let mut x = self.seed;
            x ^= x << 13;
            x ^= x >> 17;
            x ^= x << 5;
            self.seed = x;
            match x % 100 {
                0 => 300 + (x >> 8) as u64 % 400, // held off by a long critical section
                1..=9 => 20 + (x >> 8) as u64 % 60,
                _ => 2 + (x >> 8) as u64 % 6,
            }
        }

        /// The SOF record the interrupt handler leaves for frame `k`: 11-bit frame number, local µs.
        fn sof(&mut self, k: u32) -> (u16, u64) {
            let t = self.true_local(k) as u64 + self.latency();
            (((self.frame0 + k) & 0x7ff) as u16, t)
        }

        fn frame(&self, k: u32) -> u16 {
            ((self.frame0 + k) & 0x7ff) as u16
        }
    }

    /// Pushes the SOF record every `every` frames for frames `from..to`.
    fn run(track: &mut SofTrack, b: &mut Bench, from: u32, to: u32, every: u32) {
        let mut k = from;
        while k < to {
            let (f, t) = b.sof(k);
            let p = track.push(f, t);
            assert!(
                !matches!(p, Push::Restarted(_)),
                "frame {k}: {p:?} (state {:?})",
                track.state()
            );
            k += every;
        }
    }

    #[test]
    fn idle_until_a_sample() {
        let t = SofTrack::new();
        assert_eq!(t.state(), SofState::Idle);
        assert_eq!(t.local_at(3), None);
        assert_eq!(t.map(), None);
        assert_eq!(t.rate_ppb(), None);
    }

    #[test]
    fn acquiring_until_the_baseline() {
        let mut t = SofTrack::new();
        let mut b = Bench::new(0.0);
        assert_eq!(t.push(b.frame(0), b.true_local(0) as u64), Push::Started);
        run(&mut t, &mut b, 1, MIN_BASELINE as u32, 7);
        assert_eq!(t.state(), SofState::Acquiring);
        assert_eq!(t.local_at(b.frame(100)), None);
        run(
            &mut t,
            &mut b,
            MIN_BASELINE as u32,
            2 * MIN_BASELINE as u32,
            7,
        );
        assert_eq!(t.state(), SofState::Tracking);
    }

    #[test]
    fn same_sof_twice() {
        let mut t = SofTrack::new();
        t.push(10, 5_000);
        assert_eq!(t.push(10, 5_000), Push::Same);
        assert_eq!(t.push(11, 6_001), Push::Taken);
    }

    // An HSI16 2000 ppm fast (Donguru's was about that): after a minute the rate is within a ppm and every frame's
    // local time within a few µs, through 30 wraps of the 11-bit frame number.
    #[test]
    fn rc_oscillator_fitted_through_wraps() {
        for every in [1, 7, 50, 300] {
            let mut t = SofTrack::new();
            let mut b = Bench::new(2000.0);
            run(&mut t, &mut b, 0, 62_000, every);
            assert_eq!(t.state(), SofState::Tracking);
            let rate = t.rate_ppb().unwrap();
            assert!(
                (rate - 2_000_000).abs() < 1_000,
                "every {every}: {rate} ppb"
            );
            for k in [61_000, 61_500, 61_999] {
                let got = t.local_at(b.frame(k)).unwrap() as f64;
                let err = got - b.true_local(k);
                assert!(
                    (0.0..=10.0).contains(&err),
                    "every {every}, frame {k}: {err} µs"
                );
            }
            let m = t.map().unwrap();
            assert!(
                m.baseline_frames as i64 >= MAX_BASELINE / 2 - WINDOW,
                "{m:?}"
            );
        }
    }

    // Each extra half baseline moves the oldest point on, so the fit follows a rate that changes (a crystal
    // warming up).
    #[test]
    fn follows_a_rate_change() {
        let mut t = SofTrack::new();
        let mut b = Bench::new(20.0);
        run(&mut t, &mut b, 0, 70_000, 10);
        assert!((t.rate_ppb().unwrap() - 20_000).abs() < 1_000);
        // from frame 70 000 on, 25 ppm: continue the clock where it was
        let t70 = b.true_local(70_000);
        b.t0 = t70 - 70_000.0 * 1000.0 * (1.0 + 25e-6);
        b.rate = 25e-6;
        run(
            &mut t,
            &mut b,
            70_000,
            70_000 + 3 * MAX_BASELINE as u32 / 2,
            10,
        );
        assert!(
            (t.rate_ppb().unwrap() - 25_000).abs() < 1_000,
            "{:?}",
            t.rate_ppb()
        );
    }

    #[test]
    fn gap_within_the_limit_keeps_the_count() {
        let mut t = SofTrack::new();
        let mut b = Bench::new(-300.0);
        run(&mut t, &mut b, 0, 5_000, 5);
        // 7.5 s without a sample: more than three wraps of the frame number
        let k = 5_000 + 7_500;
        let (f, s) = b.sof(k);
        assert_eq!(t.push(f, s), Push::Taken);
        run(&mut t, &mut b, k + 5, k + 2_000, 5);
        let err = t.local_at(b.frame(k + 1_990)).unwrap() as f64 - b.true_local(k + 1_990);
        assert!((0.0..=10.0).contains(&err), "{err}");
    }

    #[test]
    fn long_gap_starts_over() {
        let mut t = SofTrack::new();
        let mut b = Bench::new(0.0);
        run(&mut t, &mut b, 0, 3_000, 5);
        let (f, s) = b.sof(3_000 + 9_000);
        assert_eq!(t.push(f, s), Push::Restarted(Restart::Gap));
        assert_eq!(t.state(), SofState::Acquiring);
        assert_eq!(t.restarts(), 1);
    }

    #[test]
    fn backwards_starts_over() {
        let mut t = SofTrack::new();
        t.push(1, 10_000);
        assert_eq!(t.push(2, 9_000), Push::Restarted(Restart::Backwards));
    }

    // Plugged into another host: its frame numbers have nothing to do with the old ones.
    #[test]
    fn other_host_starts_over() {
        let mut t = SofTrack::new();
        let mut b = Bench::new(100.0);
        run(&mut t, &mut b, 0, 3_000, 5);
        assert_eq!(t.state(), SofState::Tracking);
        // 400 frames on by the clock, but the new host's frame number is 300 µs into its frame
        let (_, s) = b.sof(3_400);
        let p = t.push(b.frame(3_400 + 777), s - 300);
        assert_eq!(p, Push::Restarted(Restart::Jump));
    }

    #[test]
    fn late_samples_are_no_points() {
        let mut t = SofTrack::new();
        let mut b = Bench::new(0.0);
        run(&mut t, &mut b, 0, 3_000, 5);
        let (f, s) = b.sof(3_005);
        assert_eq!(t.push(f, s + 800), Push::Late);
        // a run of them: the line is wrong
        let mut last = Push::Late;
        for i in 1..LATE_RUN as u32 {
            let (f, s) = b.sof(3_005 + 5 * i);
            last = t.push(f, s + 800);
        }
        assert_eq!(last, Push::Restarted(Restart::Jump));
    }

    #[test]
    fn steer_from_a_frame() {
        let mut t = SofTrack::new();
        let mut b = Bench::new(2000.0);
        run(&mut t, &mut b, 0, 10_000, 3);
        let k = 9_990;
        let s = t
            .steer(FrameSteer {
                frame: b.frame(k),
                shared_us: 1_761_000_000_000_000,
                rate_ppb: 1_000,
                error_us: 5,
                steerer: 7,
            })
            .unwrap();
        assert!((s.local_us as f64 - b.true_local(k)).abs() < 10.0);
        assert_eq!(
            (s.shared_us, s.error_us, s.steerer),
            (1_761_000_000_000_000, 5, 7)
        );
        // (1 + 1 ppm) / (1 + 2000 ppm) - 1 = -1995.01 ppm
        let exact = (1.0 + 1e-6) / (1.0 + t.rate_ppb().unwrap() as f64 * 1e-9) - 1.0;
        assert!((s.rate_ppb as f64 - exact * 1e9).abs() < 2.0, "{s:?}");
    }

    // The point of it all: a SharedClock steered from frames agrees with the host's frame times.
    #[test]
    fn shared_clock_on_frame_points() {
        use crate::{SharedClock, Source};
        const UTC: u64 = 1_761_000_000_000_000;
        let mut t = SofTrack::new();
        let mut b = Bench::new(-1500.0);
        let mut clock = SharedClock::default();
        // the host: frame k began at UTC + k ms (its controller exact)
        let mut k = 0;
        while k < 40_000 {
            let (f, s) = b.sof(k);
            t.push(f, s);
            if k % 1000 == 0 && t.state() == SofState::Tracking {
                let p = t
                    .steer(FrameSteer {
                        frame: b.frame(k),
                        shared_us: UTC + k as u64 * 1000,
                        rate_ppb: 0,
                        error_us: 5,
                        steerer: 0,
                    })
                    .unwrap();
                let _ = clock.steer(s, p, Source::Host);
            }
            k += 4;
        }
        let k = 39_996;
        let local = b.true_local(k) as u64;
        let shared = clock.shared(local).unwrap();
        let err = shared as i64 - (UTC + k as u64 * 1000) as i64;
        assert!(err.abs() <= 10, "{err} µs");
    }
}
