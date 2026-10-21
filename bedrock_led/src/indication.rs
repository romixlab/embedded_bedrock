//! The parts of a `ww_indication::Indication` server every board does the same way (feature `ww-indication`):
//! brightness as a peak ([Peaks]), and the end of a running identify for `Busy` ([Identify]).
//!
//! What a mode shows is the board's decision (which LEDs stay for `OnlyEssential`, what `Normal` animates), and so
//! is reading the identify duration (integer only on a Cortex-M0+, floats where they are cheap).

pub use ww_indication::{Brightness, Mode};

use crate::DEFAULT_PEAK;

/// Longest identify, longer requests are cut to it.
pub const IDENTIFY_MAX_MS: u32 = 10 * 60 * 1000;

/// Perceptual peak (see [DEFAULT_PEAK]) for each [Brightness].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Peaks {
    pub lowest: u8,
    pub low: u8,
    pub normal: u8,
    pub high: u8,
    pub highest: u8,
}

/// Peaks for a small LED behind a light pipe or seen directly on a desk.
pub const PEAKS: Peaks = Peaks {
    lowest: 8,
    low: 48,
    normal: DEFAULT_PEAK,
    high: 200,
    highest: 255,
};

impl Peaks {
    /// The peak for `brightness`; `None` for a level from a newer host (the board counts it and takes `normal`).
    pub fn peak(&self, brightness: Brightness) -> Option<u8> {
        match brightness {
            Brightness::Lowest => Some(self.lowest),
            Brightness::Low => Some(self.low),
            Brightness::Normal => Some(self.normal),
            Brightness::High => Some(self.high),
            Brightness::Highest => Some(self.highest),
            _ => None,
        }
    }
}

/// A point in device time the identify end is kept in: ms as `u64`, or `embassy_time::Instant` (feature `embassy`,
/// no 64-bit division on the way).
pub trait Timestamp: Copy + Ord {
    const ZERO: Self;
    fn add_ms(self, ms: u32) -> Self;
}

impl Timestamp for u64 {
    const ZERO: Self = 0;
    fn add_ms(self, ms: u32) -> Self {
        self + ms as u64
    }
}

#[cfg(feature = "embassy")]
impl Timestamp for embassy_time::Instant {
    const ZERO: Self = embassy_time::Instant::from_ticks(0);
    fn add_ms(self, ms: u32) -> Self {
        self + embassy_time::Duration::from_millis(ms as u64)
    }
}

/// The end of a running identify, for `Busy`. Kept next to the mode and brightness the API last set (RAM only:
/// `Normal` after a reset until the board has settings).
#[derive(Clone, Copy, Debug)]
pub struct Identify<T> {
    until: T,
}

impl<T: Timestamp> Identify<T> {
    pub const fn new() -> Self {
        Self { until: T::ZERO }
    }

    /// Whether an identify started through [start](Self::start) still runs at `now`: the call answers `Busy`.
    pub fn busy(&self, now: T) -> bool {
        now < self.until
    }

    /// Starts an identify of `ms` at `now`, cut to [IDENTIFY_MAX_MS]: returns how long to blink.
    pub fn start(&mut self, now: T, ms: u32) -> u32 {
        let ms = ms.min(IDENTIFY_MAX_MS);
        self.until = now.add_ms(ms);
        ms
    }
}

impl<T: Timestamp> Default for Identify<T> {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn peaks_rise() {
        let levels = [
            Brightness::Lowest,
            Brightness::Low,
            Brightness::Normal,
            Brightness::High,
            Brightness::Highest,
        ]
        .map(|b| PEAKS.peak(b).unwrap());
        assert!(levels.windows(2).all(|w| w[0] < w[1]));
        assert_eq!(PEAKS.peak(Brightness::Normal), Some(DEFAULT_PEAK));
        assert_eq!(PEAKS.peak(Brightness::Highest), Some(255));
    }

    #[test]
    fn identify_busy_and_capped() {
        let mut i = Identify::<u64>::new();
        assert!(!i.busy(0));
        assert_eq!(i.start(1000, 5000), 5000);
        assert!(i.busy(5999));
        assert!(!i.busy(6000));
        assert_eq!(i.start(6000, 3_600_000), IDENTIFY_MAX_MS);
        assert!(i.busy(6000 + IDENTIFY_MAX_MS as u64 - 1));
        // 0 ends nothing and blocks nothing
        assert_eq!(i.start(30_000_000, 0), 0);
        assert!(!i.busy(30_000_000));
    }
}
