//! The parts of a `ww_timebase::Clock` server every board does the same way (feature `ww-timebase`): steer points
//! in, `sync_status`, `time_map` points and `shared_now` out, the sync line's configuration kept.
//!
//! What `now()` and `info()` answer is the board's (its time format, its clock's name); the board passes how it
//! writes a local time as a [Second] (the same function its `now()` uses), so `time_map` and `sync_status` come in
//! the same prefix and storage. Driving a sync line (IRIG-B, PPS) is the board's too: it steers the same
//! [SharedClock] with [Source::Line] points.

use wire_weaver::prelude::*;
use ww_si::Prefix;
use ww_timebase::{
    ClockError, Second, SteerPoint, SyncConfig, SyncFormat, SyncSource, SyncState, SyncStatus,
    TimeMapPoint,
};

use crate::{SharedClock, Source, State, Steer, SteerError, Steered};

/// The `Clock` server's state besides the [SharedClock]: the sync line's configuration and how the board writes
/// local time.
#[derive(Copy, Clone, Debug)]
pub struct ClockServer {
    config: SyncConfig,
    to_second: fn(u64) -> Second,
}

impl ClockServer {
    /// `to_second` writes local µs as the board's `now()` does (a `U32` of µs on a Cortex-M0+, wrapping).
    pub const fn new(to_second: fn(u64) -> Second) -> Self {
        ClockServer {
            config: SyncConfig::Off,
            to_second,
        }
    }

    /// `Clock::configure_sync`: `Off` always, a master or slave in one of the formats the board's line supports,
    /// `Unsupported` otherwise.
    pub fn configure_sync(
        &mut self,
        config: SyncConfig,
        supported: &[SyncFormat],
    ) -> Result<(), ClockError<'static>> {
        match config {
            SyncConfig::Off => {}
            SyncConfig::Master { format } | SyncConfig::Slave { format }
                if supported.contains(&format) => {}
            _ => return Err(CommonError::Unsupported.into()),
        }
        self.config = config;
        Ok(())
    }

    /// `Clock::sync_config`.
    pub fn sync_config(&self) -> SyncConfig {
        self.config
    }

    /// `Clock::steer` at local time `now_us`: the point's `local` read near `now_us` (any integer storage of
    /// `Prefix::Micro`, 32 bits are enough), then [SharedClock::steer] as [Source::Host].
    pub fn steer(
        &self,
        clock: &mut SharedClock,
        now_us: u64,
        point: &SteerPoint,
    ) -> Result<SyncSource, ClockError<'static>> {
        let invalid = |reason| -> ClockError<'static> {
            CommonError::InvalidArgument {
                arg: 0,
                reason: MaxLen::new_unchecked(reason),
            }
            .into()
        };
        if point.local.prefix != Prefix::Micro {
            return Err(invalid("local not in µs"));
        }
        let low = point
            .local
            .to_u32_wrapping_in(Prefix::Micro)
            .ok_or_else(|| invalid("local not an integer"))?;
        let steer = Steer {
            local_us: unwrap_near(low, now_us),
            shared_us: point.shared_us,
            rate_ppb: point.rate_ppb,
            error_us: point.error_us,
        };
        match clock.steer(now_us, steer, Source::Host) {
            Ok(Steered::Ignored(source)) => Ok(self.source(source)),
            Ok(_) => Ok(SyncSource::Host),
            Err(SteerError::RateOutOfRange) => Err(invalid("rate beyond 3 %")),
        }
    }

    /// `Clock::sync_status` at local time `now_us`.
    pub fn sync_status(&self, clock: &SharedClock, now_us: u64) -> SyncStatus {
        SyncStatus {
            state: state(clock.state(now_us)),
            // local minus shared doesn't fit a device timestamp's storage when shared time is UTC
            offset: None,
            last_lock: clock.last_point().map(self.to_second),
            source: clock.source().map(|s| self.source(s)),
            error: clock.error_us(now_us).map(|us| {
                Second::from_parts(Prefix::Micro, ww_si::ww_numeric::NumericValue::U32(us))
            }),
        }
    }

    /// A `time_map` point at local time `now_us` (about once a second while synced, and when the state or source
    /// changed), `None` before the first steer point. `shared` is written as the board writes local time.
    pub fn time_map_point(&self, clock: &mut SharedClock, now_us: u64) -> Option<TimeMapPoint> {
        let shared = clock.shared(now_us)?;
        Some(TimeMapPoint {
            local: (self.to_second)(now_us),
            shared: (self.to_second)(shared),
            state: state(clock.state(now_us)),
            source: clock.source().map(|s| self.source(s)),
        })
    }

    /// `Clock::shared_now` at local time `now_us`.
    pub fn shared_now(&self, clock: &mut SharedClock, now_us: u64) -> Option<u64> {
        clock.shared(now_us)
    }

    /// `info().sync_formats` of a board without a sync line: none (host steering needs no format).
    pub const NO_LINE: &'static [SyncFormat] = &[];

    fn source(&self, source: Source) -> SyncSource {
        match (source, self.config) {
            (Source::Line, SyncConfig::Slave { format } | SyncConfig::Master { format }) => {
                SyncSource::Line { format }
            }
            // a line source without a line configured: the board steered it with its own format
            (Source::Line, _) => SyncSource::Line {
                format: SyncFormat::Pps,
            },
            (Source::Host, _) => SyncSource::Host,
        }
    }
}

fn state(s: State) -> SyncState {
    match s {
        State::Free => SyncState::Free,
        State::Locked => SyncState::Locked,
        State::Holdover => SyncState::Holdover,
    }
}

/// The full local time whose low 32 bits are `low`, nearest to `now_us` (within ±35 minutes).
fn unwrap_near(low: u32, now_us: u64) -> u64 {
    let back = (now_us as u32).wrapping_sub(low) as i32;
    now_us.wrapping_sub(back as i64 as u64)
}

#[cfg(test)]
mod tests {
    use super::*;
    use ww_si::ww_numeric::NumericValue;

    fn micros(us: u64) -> Second {
        Second::from_parts(Prefix::Micro, NumericValue::U32(us as u32))
    }

    #[test]
    fn unwraps_near_now() {
        assert_eq!(unwrap_near(5, 1 << 32), (1 << 32) + 5);
        assert_eq!(unwrap_near(u32::MAX, (1 << 32) + 3), (1 << 32) - 1);
        assert_eq!(unwrap_near(100, 90), 100);
    }

    #[test]
    fn steer_status_and_time_map() {
        let server = ClockServer::new(micros);
        let mut clock = SharedClock::default();
        let now = (1u64 << 32) + 1_000;
        assert_eq!(server.sync_status(&clock, now).state, SyncState::Free);
        assert!(server.time_map_point(&mut clock, now).is_none());
        let point = SteerPoint {
            local: micros(now - 500),
            shared_us: 1_761_000_000_000_000,
            rate_ppb: 0,
            error_us: 120,
        };
        assert_eq!(server.steer(&mut clock, now, &point), Ok(SyncSource::Host));
        assert_eq!(
            server.shared_now(&mut clock, now),
            Some(1_761_000_000_000_500)
        );
        let status = server.sync_status(&clock, now);
        assert_eq!(status.state, SyncState::Locked);
        assert_eq!(status.source, Some(SyncSource::Host));
        assert_eq!(status.error, Some(micros(120)));
        let p = server.time_map_point(&mut clock, now).unwrap();
        assert_eq!(p.source, Some(SyncSource::Host));
        assert_eq!(p.local, micros(now));
        // a point in ms, or a rate too far off
        let bad = SteerPoint {
            local: Second::from_parts(Prefix::Milli, NumericValue::U32(1)),
            ..point
        };
        assert!(server.steer(&mut clock, now, &bad).is_err());
        let fast = SteerPoint {
            rate_ppb: 50_000_000,
            ..point
        };
        assert!(server.steer(&mut clock, now, &fast).is_err());
    }

    #[test]
    fn configure_and_line_source() {
        let mut server = ClockServer::new(micros);
        assert_eq!(
            server.configure_sync(
                SyncConfig::Slave {
                    format: SyncFormat::IrigB
                },
                ClockServer::NO_LINE
            ),
            Err(CommonError::Unsupported.into())
        );
        server
            .configure_sync(
                SyncConfig::Slave {
                    format: SyncFormat::IrigB,
                },
                &[SyncFormat::IrigB],
            )
            .unwrap();
        let mut clock = SharedClock::default();
        let line = Steer {
            local_us: 0,
            shared_us: 1_761_000_000_000_000,
            rate_ppb: 0,
            error_us: 2,
        };
        clock.steer(0, line, Source::Line).unwrap();
        let point = SteerPoint {
            local: micros(10),
            shared_us: 1_761_000_000_000_300,
            rate_ppb: 0,
            error_us: 100,
        };
        // the host is only noted while the line is locked
        assert_eq!(
            server.steer(&mut clock, 10, &point),
            Ok(SyncSource::Line {
                format: SyncFormat::IrigB
            })
        );
        assert_eq!(clock.shared(10), Some(1_761_000_000_000_010));
    }
}
