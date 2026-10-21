//! The parts of a `ww_timebase::Clock` server every board does the same way (feature `ww-timebase`): steer points
//! in, `sync_status`, `time_map` points and `shared_now` out, the sync line's configuration kept.
//!
//! What `now()` and `info()` answer is the board's (its time format, its clock's name); the board passes how it
//! writes a local time as a [Second] (the same function its `now()` uses), so `time_map` and `sync_status` come in
//! the same prefix and storage. Driving a sync line (IRIG-B, PPS) is the board's too: it steers the same
//! [SharedClock] with [Source::Line] points.
//!
//! Steer points name their steerer (`SteerPoint::steerer`): the [SharedClock] follows one at a time, and the
//! server keeps the followed and the last noted one (id, pid, host and program cut to [NAME_MAX] bytes) for
//! `sync_status`.
//!
//! `set_sources` / `sources` keep the explicit source list in both forms: the [SharedClock]'s, which decides, and
//! the `ww_timebase` one the board answers `sources()` with.

use wire_weaver::prelude::*;
use ww_si::Prefix;
use ww_timebase::{
    ClockError, Second, SteerPoint, Steerer, SyncConfig, SyncFormat, SyncSource, SyncState,
    SyncStatus, TimeMapPoint,
};

use crate::{
    Line, NoOscillator, Oscillator, SharedClock, Source, Sources, SourcesError, State, Steer,
    SteerError, Steered,
};

/// Longest host and program name kept of a steerer, bytes: longer ones are cut at a char boundary.
pub const NAME_MAX: usize = 24;

/// A name of up to [NAME_MAX] bytes, cut at a char boundary.
#[derive(Copy, Clone, Debug)]
struct Name {
    buf: [u8; NAME_MAX],
    len: u8,
}

impl Name {
    const EMPTY: Name = Name {
        buf: [0; NAME_MAX],
        len: 0,
    };

    fn set(&mut self, s: &str) {
        let mut len = s.len().min(NAME_MAX);
        while !s.is_char_boundary(len) {
            len -= 1;
        }
        self.buf[..len].copy_from_slice(&s.as_bytes()[..len]);
        self.len = len as u8;
    }

    fn as_str(&self) -> &str {
        // only ever whole chars of a str
        core::str::from_utf8(&self.buf[..self.len as usize]).unwrap_or("")
    }
}

/// A steerer as `sync_status` reports it, in fixed buffers.
#[derive(Copy, Clone, Debug)]
struct Kept {
    id: u64,
    pid: u32,
    host: Name,
    program: Name,
}

impl Kept {
    const NONE: Kept = Kept {
        id: 0,
        pid: 0,
        host: Name::EMPTY,
        program: Name::EMPTY,
    };

    /// Keeps `steerer`, `id` 0 (nobody) for `None`. The names are copied only when the id changes.
    fn set(&mut self, steerer: Option<&Steerer>) {
        let Some(s) = steerer else {
            self.id = 0;
            return;
        };
        if s.id != self.id {
            self.id = s.id;
            self.pid = s.pid;
            self.host.set(s.host);
            self.program.set(s.program);
        }
    }

    fn get(&self) -> Option<Steerer<'_>> {
        (self.id != 0).then(|| Steerer {
            id: self.id,
            host: self.host.as_str(),
            program: self.program.as_str(),
            pid: self.pid,
        })
    }
}

/// The `Clock` server's state besides the [SharedClock]: the sync line's configuration, how the board writes local
/// time and the steerers seen.
#[derive(Copy, Clone, Debug)]
pub struct ClockServer {
    config: SyncConfig,
    to_second: fn(u64) -> Second,
    /// The explicit source list as `sources()` answers it, `None` for the fixed ranking.
    sources: Option<([SyncSource; Sources::MAX], u8)>,
    /// The steerer followed or last followed, as its points named it.
    steerer: Kept,
    /// The last other steerer only noted, and the local time of its last point.
    noted: Kept,
    noted_at: u64,
}

impl ClockServer {
    /// `to_second` writes local µs as the board's `now()` does (a `U32` of µs on a Cortex-M0+, wrapping).
    pub const fn new(to_second: fn(u64) -> Second) -> Self {
        ClockServer {
            config: SyncConfig::Off,
            to_second,
            sources: None,
            steerer: Kept::NONE,
            noted: Kept::NONE,
            noted_at: 0,
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
    /// `Prefix::Micro`, 32 bits are enough), then [SharedClock::steer] as [Source::Host] from the point's steerer.
    /// A point from another steerer than the one followed answers `Host` too, and is kept as `noted`.
    pub fn steer(
        &mut self,
        clock: &mut SharedClock,
        now_us: u64,
        point: &SteerPoint,
    ) -> Result<Option<SyncSource>, ClockError<'static>> {
        self.steer_with(clock, now_us, point, &mut NoOscillator)
    }

    /// [steer](Self::steer) on a board with an [Oscillator] backend: [SharedClock::steer_with] trims `osc` by part
    /// of the point's rate.
    pub fn steer_with(
        &mut self,
        clock: &mut SharedClock,
        now_us: u64,
        point: &SteerPoint,
        osc: &mut impl Oscillator,
    ) -> Result<Option<SyncSource>, ClockError<'static>> {
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
            steerer: point.steerer.as_ref().map_or(0, |s| s.id),
        };
        match clock.steer_with(now_us, steer, Source::Host, osc) {
            Ok(Steered::Ignored(source)) => Ok(source.map(sync_source)),
            Ok(Steered::OtherSteerer) => {
                self.noted.set(point.steerer.as_ref());
                self.noted_at = now_us;
                Ok(Some(SyncSource::Host))
            }
            Ok(_) => {
                self.steerer.set(point.steerer.as_ref());
                if self.noted.id == steer.steerer {
                    self.noted.id = 0;
                }
                Ok(Some(SyncSource::Host))
            }
            Err(SteerError::RateOutOfRange) => Err(invalid("rate beyond 3 %")),
        }
    }

    /// `Clock::sync_status` at local time `now_us`: `steerer` while the clock follows (or last followed) the host,
    /// `noted` while the other steerer's last point is within [Config::holdover_after_us](crate::Config).
    pub fn sync_status(&self, clock: &SharedClock, now_us: u64) -> SyncStatus<'_> {
        let state = clock.state(now_us);
        let hosted = clock.source() == Some(Source::Host) && state != State::Free;
        let noted_lately = now_us.saturating_sub(self.noted_at) <= clock.config().holdover_after_us;
        SyncStatus {
            state: sync_state(state),
            // local minus shared doesn't fit a device timestamp's storage when shared time is UTC
            offset: None,
            last_lock: clock.last_point().map(self.to_second),
            source: clock.source().map(sync_source),
            error: clock.error_us(now_us).map(|us| {
                Second::from_parts(Prefix::Micro, ww_si::ww_numeric::NumericValue::U32(us))
            }),
            slew_us: clock.last_point().map(|_| {
                clock
                    .slew_left_at(now_us)
                    .clamp(i32::MIN as i64, i32::MAX as i64) as i32
            }),
            steerer: if hosted { self.steerer.get() } else { None },
            noted: if hosted && noted_lately {
                self.noted.get()
            } else {
                None
            },
        }
    }

    /// A `time_map` point at local time `now_us` (about once a second while synced, and when the state or source
    /// changed), `None` before the first steer point. `shared` is written as the board writes local time.
    pub fn time_map_point(&self, clock: &mut SharedClock, now_us: u64) -> Option<TimeMapPoint> {
        let shared = clock.shared(now_us)?;
        Some(TimeMapPoint {
            local: (self.to_second)(now_us),
            shared: (self.to_second)(shared),
            state: sync_state(clock.state(now_us)),
            source: clock.source().map(sync_source),
        })
    }

    /// `Clock::shared_now` at local time `now_us`.
    pub fn shared_now(&self, clock: &mut SharedClock, now_us: u64) -> Option<u64> {
        clock.shared(now_us)
    }

    /// `info().sync_formats` of a board without a sync line: none (host steering needs no format).
    pub const NO_LINE: &'static [SyncFormat] = &[];

    /// `Clock::set_sources`: the list, best first, or `None` for the fixed ranking (a locked line before the host).
    /// `Unsupported` for a line format not in `supported` (the board's `info().sync_formats`), `InvalidArgument` for
    /// a source named twice, more than [Sources::MAX] of them, or one from a newer API.
    pub fn set_sources(
        &mut self,
        clock: &mut SharedClock,
        sources: Option<MaxLen<RefVec<'_, SyncSource>, 8>>,
        supported: &[SyncFormat],
    ) -> Result<(), ClockError<'static>> {
        let invalid = |reason| -> ClockError<'static> {
            CommonError::InvalidArgument {
                arg: 0,
                reason: MaxLen::new_unchecked(reason),
            }
            .into()
        };
        let Some(sources) = sources else {
            clock.set_sources(None);
            self.sources = None;
            return Ok(());
        };
        let mut wire = [SyncSource::Host; Sources::MAX];
        let mut list = [Source::Host; Sources::MAX];
        let mut len = 0;
        for s in sources.iter() {
            let s = s.map_err(|_| invalid("a source list that doesn't read"))?;
            if len == Sources::MAX {
                return Err(invalid("more than 4 sources"));
            }
            list[len] = match s {
                SyncSource::Host => Source::Host,
                SyncSource::Line { format } if !supported.contains(&format) => {
                    return Err(CommonError::Unsupported.into());
                }
                SyncSource::Line {
                    format: SyncFormat::IrigB,
                } => Source::Line(Line::IrigB),
                SyncSource::Line {
                    format: SyncFormat::Pps,
                } => Source::Line(Line::Pps),
                _ => return Err(invalid("a source from a newer version")),
            };
            wire[len] = s;
            len += 1;
        }
        let checked = Sources::new(&list[..len]).map_err(|e| match e {
            SourcesError::TooMany => invalid("more than 4 sources"),
            SourcesError::Twice => invalid("a source named twice"),
        })?;
        clock.set_sources(Some(checked));
        self.sources = Some((wire, len as u8));
        Ok(())
    }

    /// `Clock::sources`.
    pub fn sources(&self) -> Option<MaxLen<RefVec<'_, SyncSource>, 8>> {
        self.sources.as_ref().map(|(list, len)| {
            MaxLen::new_unchecked(RefVec::Slice {
                slice: &list[..*len as usize],
            })
        })
    }
}

/// `source` as `ww_timebase` names it.
pub fn sync_source(source: Source) -> SyncSource {
    match source {
        Source::Line(Line::IrigB) => SyncSource::Line {
            format: SyncFormat::IrigB,
        },
        Source::Line(Line::Pps) => SyncSource::Line {
            format: SyncFormat::Pps,
        },
        Source::Host => SyncSource::Host,
    }
}

fn sync_state(s: State) -> SyncState {
    match s {
        State::Free => SyncState::Free,
        State::Locked => SyncState::Locked,
        State::Acquiring => SyncState::Acquiring,
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
        let mut server = ClockServer::new(micros);
        let mut clock = SharedClock::default();
        let now = (1u64 << 32) + 1_000;
        assert_eq!(server.sync_status(&clock, now).state, SyncState::Free);
        assert!(server.time_map_point(&mut clock, now).is_none());
        let point = SteerPoint {
            local: micros(now - 500),
            shared_us: 1_761_000_000_000_000,
            rate_ppb: 0,
            error_us: 120,
            steerer: None,
        };
        assert_eq!(
            server.steer(&mut clock, now, &point),
            Ok(Some(SyncSource::Host))
        );
        assert_eq!(
            server.shared_now(&mut clock, now),
            Some(1_761_000_000_000_500)
        );
        let status = server.sync_status(&clock, now);
        assert_eq!(status.state, SyncState::Locked);
        assert_eq!(status.source, Some(SyncSource::Host));
        assert_eq!(status.error, Some(micros(120)));
        assert_eq!(status.slew_us, Some(0));
        let p = server.time_map_point(&mut clock, now).unwrap();
        assert_eq!(p.source, Some(SyncSource::Host));
        assert_eq!(p.local, micros(now));
        // a point in ms, or a rate too far off
        let bad = SteerPoint {
            local: Second::from_parts(Prefix::Milli, NumericValue::U32(1)),
            ..point.clone()
        };
        assert!(server.steer(&mut clock, now, &bad).is_err());
        let fast = SteerPoint {
            rate_ppb: 50_000_000,
            ..point.clone()
        };
        assert!(server.steer(&mut clock, now, &fast).is_err());
    }

    #[test]
    fn steer_with_trims_the_oscillator() {
        struct Trim(i32);
        impl crate::Oscillator for Trim {
            fn set_trim_ppb(&mut self, ppb: i32) -> i32 {
                self.0 = ppb;
                ppb
            }
        }
        let mut server = ClockServer::new(micros);
        let mut clock = SharedClock::default();
        let mut osc = Trim(0);
        let point = SteerPoint {
            local: micros(1_000),
            shared_us: 1_761_000_000_000_000,
            rate_ppb: 6_400,
            error_us: 120,
            steerer: None,
        };
        assert_eq!(
            server.steer_with(&mut clock, 1_000, &point, &mut osc),
            Ok(Some(SyncSource::Host))
        );
        assert_eq!(osc.0, 100);
        assert_eq!(clock.rates(), (6_400, 100));
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
            steerer: 0,
        };
        clock.steer(0, line, Source::Line(Line::IrigB)).unwrap();
        let point = SteerPoint {
            local: micros(10),
            shared_us: 1_761_000_000_000_300,
            rate_ppb: 0,
            error_us: 100,
            steerer: None,
        };
        // the host is only noted while the line is locked
        assert_eq!(
            server.steer(&mut clock, 10, &point),
            Ok(Some(SyncSource::Line {
                format: SyncFormat::IrigB
            }))
        );
        assert_eq!(clock.shared(10), Some(1_761_000_000_000_010));
    }

    #[test]
    fn source_list_in_both_forms() {
        let mut server = ClockServer::new(micros);
        let mut clock = SharedClock::default();
        assert!(server.sources().is_none());
        let irig = SyncSource::Line {
            format: SyncFormat::IrigB,
        };
        let only_line = [irig];
        let list = || {
            MaxLen::new_unchecked(RefVec::Slice {
                slice: &only_line[..],
            })
        };
        assert_eq!(
            server.set_sources(&mut clock, Some(list()), ClockServer::NO_LINE),
            Err(CommonError::Unsupported.into())
        );
        server
            .set_sources(&mut clock, Some(list()), &[SyncFormat::IrigB])
            .unwrap();
        let back: Vec<_> = server
            .sources()
            .unwrap()
            .iter()
            .map(|s| s.unwrap())
            .collect();
        assert_eq!(back, [irig]);
        assert_eq!(clock.sources(), Some(&[Source::Line(Line::IrigB)][..]));
        // the host is only noted, nothing followed
        let point = SteerPoint {
            local: micros(10),
            shared_us: 1_761_000_000_000_300,
            rate_ppb: 0,
            error_us: 100,
            steerer: None,
        };
        assert_eq!(server.steer(&mut clock, 10, &point), Ok(None));
        let twice = [SyncSource::Host, SyncSource::Host];
        let r = server.set_sources(
            &mut clock,
            Some(MaxLen::new_unchecked(RefVec::Slice { slice: &twice[..] })),
            &[],
        );
        assert!(r.is_err());
        server.set_sources(&mut clock, None, &[]).unwrap();
        assert!(server.sources().is_none());
        assert_eq!(
            server.steer(&mut clock, 10, &point),
            Ok(Some(SyncSource::Host))
        );
    }

    #[test]
    fn status_names_the_followed_and_the_noted_steerer() {
        let mut server = ClockServer::new(micros);
        let mut clock = SharedClock::default();
        let hub = Steerer {
            id: 11,
            host: "omarchy-m1",
            program: "ww hub",
            pid: 4242,
        };
        // 27 bytes, an "é" across byte 24: cut before it
        let long = "a-very-long-host-name-xé-yz";
        let app = Steerer {
            id: 22,
            host: long,
            program: "app",
            pid: 7,
        };
        let at = |us: u64, steerer: &Steerer<'static>| SteerPoint {
            local: micros(us),
            shared_us: 1_761_000_000_000_000 + us,
            rate_ppb: 0,
            error_us: 100,
            steerer: Some(steerer.clone()),
        };
        assert_eq!(
            server.steer(&mut clock, 0, &at(0, &hub)),
            Ok(Some(SyncSource::Host))
        );
        let status = server.sync_status(&clock, 0);
        assert_eq!(status.steerer, Some(hub.clone()));
        assert_eq!(status.noted, None);
        // the app's point is only noted, but answers Host
        assert_eq!(
            server.steer(&mut clock, 1_000_000, &at(1_000_000, &app)),
            Ok(Some(SyncSource::Host))
        );
        let status = server.sync_status(&clock, 1_000_000);
        assert_eq!(status.steerer, Some(hub.clone()));
        let noted = status.noted.unwrap();
        assert_eq!((noted.id, noted.pid, noted.program), (22, 7, "app"));
        assert_eq!(noted.host, "a-very-long-host-name-x");
        // the hub keeps steering, the app stops: noted no more after the holdover time
        server
            .steer(&mut clock, 4_000_000, &at(4_000_000, &hub))
            .unwrap();
        let status = server.sync_status(&clock, 4_000_001);
        assert_eq!(status.steerer.unwrap().id, 11);
        assert_eq!(status.noted, None);
        // the hub stops: holdover still names it, then the app takes over
        let status = server.sync_status(&clock, 7_500_000);
        assert_eq!(status.state, SyncState::Holdover);
        assert_eq!(status.steerer.unwrap().id, 11);
        server
            .steer(&mut clock, 7_500_000, &at(7_500_000, &app))
            .unwrap();
        let status = server.sync_status(&clock, 7_500_000);
        assert_eq!(status.steerer.unwrap().id, 22);
        // anonymous points: followed, nobody named
        let mut server = ClockServer::new(micros);
        let mut clock = SharedClock::default();
        let anon = SteerPoint {
            steerer: None,
            ..at(0, &hub)
        };
        server.steer(&mut clock, 0, &anon).unwrap();
        server.steer(&mut clock, 1_000, &at(1_000, &hub)).unwrap();
        let status = server.sync_status(&clock, 1_000);
        assert_eq!(status.steerer, None);
        assert_eq!(status.noted.unwrap().id, 11);
    }
}
