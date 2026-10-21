use super::*;

const UTC: u64 = 1_761_000_000_000_000;

fn point(local_us: u64, shared_us: u64, rate_ppb: i32) -> Steer {
    Steer {
        local_us,
        shared_us,
        rate_ppb,
        error_us: 100,
    }
}

/// Shared time over `ticks` of 10 ms from `from`, as the LED task reads it.
fn ticks(clock: &mut SharedClock, from: u64, ticks: u64) -> Vec<u64> {
    (0..=ticks)
        .map(|i| clock.shared(from + i * 10_000).unwrap())
        .collect()
}

#[test]
fn free_until_the_first_point_then_stepped() {
    let mut c = SharedClock::default();
    assert_eq!(c.shared(5), None);
    assert_eq!(c.state(5), State::Free);
    assert_eq!(c.error_us(5), None);
    let g = c.generation();
    assert_eq!(
        c.steer(1_000, point(900, UTC, 0), Source::Host),
        Ok(Steered::Stepped)
    );
    assert_ne!(c.generation(), g);
    assert_eq!(c.shared(1_000), Some(UTC + 100));
    assert_eq!(c.state(1_000), State::Locked);
    assert_eq!(c.source(), Some(Source::Host));
    assert_eq!(c.error_us(1_000), Some(100));
}

#[test]
fn follows_the_rate() {
    // an HSI 1 % fast: shared runs 1/1.01 of local
    let rate = ((1.0 / 1.01 - 1.0) * 1e9) as i32;
    let mut c = SharedClock::default();
    c.steer(0, point(0, UTC, rate), Source::Host).unwrap();
    let t = ticks(&mut c, 0, 100);
    let want = 1_000_000.0 / 1.01;
    // to the rate's Q20 step (1 ppm) over a second
    assert!((t[100] - UTC) as f64 - want < 2.0, "{}", t[100] - UTC);
    assert!(
        ((t[100] - UTC) as f64 - want).abs() < 2.0,
        "{}",
        t[100] - UTC
    );
    // monotonic, 9.9 ms a tick
    assert!(
        t.windows(2)
            .all(|w| (9_900..=9_901).contains(&(w[1] - w[0])))
    );
}

#[test]
fn slews_a_small_offset_and_steps_a_large_one() {
    let mut c = SharedClock::default();
    c.steer(0, point(0, UTC, 0), Source::Host).unwrap();
    ticks(&mut c, 0, 100);
    let g = c.generation();
    // the host says we're 5 ms behind
    let s = c.steer(
        1_000_000,
        point(1_000_000, UTC + 1_005_000, 0),
        Source::Host,
    );
    assert_eq!(s, Ok(Steered::Slewing(5_000)));
    assert_eq!(c.generation(), g, "a slew keeps the generation");
    assert_eq!(c.error_us(1_000_000), Some(5_100));
    // 5 ms off: out of lock, slewed at the acquiring rate
    assert_eq!(c.state(1_000_000), State::Acquiring);
    let t = ticks(&mut c, 1_000_000, 100);
    // 5 % of 10 ms a tick: 10.5 ms ticks until the 5 ms are in (a tenth of a second), never a jump
    assert!(
        t.windows(2)
            .all(|w| (10_000..=10_500).contains(&(w[1] - w[0])))
    );
    assert_eq!(t[100], UTC + 2_005_000);
    assert_eq!(c.error_us(2_000_000), Some(100));
    assert_eq!(c.state(2_000_000), State::Locked);
    // 2 s off: stepped, the generation changes
    let s = c.steer(
        2_000_000,
        point(2_000_000, UTC + 4_005_000, 0),
        Source::Host,
    );
    assert_eq!(s, Ok(Steered::Stepped));
    assert_ne!(c.generation(), g);
    assert_eq!(c.shared(2_000_000), Some(UTC + 4_005_000));
}

#[test]
fn holdover_then_free_with_growing_error() {
    let mut c = SharedClock::default();
    c.steer(0, point(0, UTC, 0), Source::Host).unwrap();
    assert_eq!(c.state(3_000_000), State::Locked);
    assert_eq!(c.state(3_000_001), State::Holdover);
    // 10 ppm of 100 s of holdover
    assert_eq!(c.error_us(103_000_000), Some(100 + 1_000));
    assert_eq!(c.state(3_603_000_000), State::Holdover);
    assert_eq!(c.state(3_603_000_001), State::Free);
    // still running on the last line
    assert_eq!(c.shared(3_603_000_001), Some(UTC + 3_603_000_001));
    // a point brings it back, slewed
    let s = c.steer(
        3_603_000_002,
        point(3_603_000_002, UTC + 3_603_000_502, 0),
        Source::Host,
    );
    assert_eq!(s, Ok(Steered::Slewing(500)));
    assert_eq!(c.state(3_603_000_002), State::Locked);
}

#[test]
fn a_locked_line_beats_the_host() {
    let mut c = SharedClock::default();
    assert_eq!(
        c.steer(0, point(0, UTC, 0), Source::Line(Line::IrigB)),
        Ok(Steered::Stepped)
    );
    let g = c.generation();
    assert_eq!(
        c.steer(
            1_000_000,
            point(1_000_000, UTC + 1_000_300, 0),
            Source::Host
        ),
        Ok(Steered::Ignored(Some(Source::Line(Line::IrigB))))
    );
    assert_eq!(c.shared(1_000_000), Some(UTC + 1_000_000));
    // the line is lost: the host takes over, slewed, and the switch shows in the generation
    let s = c.steer(
        4_000_000,
        point(4_000_000, UTC + 4_000_300, 0),
        Source::Host,
    );
    assert_eq!(s, Ok(Steered::Slewing(300)));
    assert_eq!(c.source(), Some(Source::Host));
    assert_ne!(c.generation(), g);
    // and the line back beats the host again
    assert_eq!(
        c.steer(
            5_000_000,
            point(5_000_000, UTC + 5_000_000, 0),
            Source::Line(Line::IrigB)
        ),
        Ok(Steered::Slewing(-300))
    );
    assert_eq!(c.source(), Some(Source::Line(Line::IrigB)));
}

#[test]
fn a_long_gap_matches_the_ticks() {
    let rate = -9_900_990;
    let mut ticked = SharedClock::default();
    let mut slept = SharedClock::default();
    for c in [&mut ticked, &mut slept] {
        c.steer(0, point(0, UTC, rate), Source::Host).unwrap();
        c.steer(1, point(1, UTC + 4_000, rate), Source::Host)
            .unwrap();
    }
    // three hours, slewing 4 ms in on the way
    let end = 3 * 3_600_000_000 + 12_345;
    let mut t = 1;
    while t < end {
        t = (t + 10_000).min(end);
        ticked.shared(t);
    }
    let a = ticked.shared(end).unwrap();
    let b = slept.shared(end).unwrap();
    assert!(a.abs_diff(b) <= 1, "{a} vs {b}");
    let want = UTC as f64 + 4_000.0 + (end as f64) * (1.0 + rate as f64 * 1e-9);
    // the rate's Q20 step: up to 0.5 ppm of three hours
    assert!((a as f64 - want).abs() < 6_000.0, "{}", a as f64 - want);
}

#[test]
fn rate_out_of_range_is_refused() {
    let mut c = SharedClock::default();
    assert_eq!(
        c.steer(0, point(0, UTC, 40_000_000), Source::Host),
        Err(SteerError::RateOutOfRange)
    );
    assert_eq!(c.shared(0), None);
    assert!(c.steer(0, point(0, UTC, -29_000_000), Source::Host).is_ok());
}

#[test]
fn local_at_inverts_shared() {
    let mut c = SharedClock::default();
    c.steer(0, point(0, UTC, -9_900_990), Source::Host).unwrap();
    c.steer(10, point(10, UTC + 2_000, -9_900_990), Source::Host)
        .unwrap();
    for target in [UTC + 5_000, UTC + 1_000_000, UTC + 3_600_000_000] {
        let local = c.local_at(target).unwrap();
        let mut probe = c.clone();
        let reached = probe.shared(local).unwrap();
        // the slew is counted as if already in: within it while it still runs, to a few µs once it has
        let tolerance = if target < UTC + 1_000_000 { 2_000 } else { 30 };
        assert!(reached.abs_diff(target) <= tolerance, "{target}: {reached}");
    }
    // in the past: now
    assert_eq!(c.local_at(UTC - 5), Some(10));
}

/// A PLL 50 ppm slow, trimmed through [Oscillator] by a host whose rate lags by its 30 s window.
#[test]
fn oscillator_takes_the_rate_without_oscillating() {
    struct Pll {
        trim: i32,
    }
    impl Oscillator for Pll {
        fn set_trim_ppb(&mut self, ppb: i32) -> i32 {
            // FRACN steps of ~30 ppb, ±1000 ppm
            self.trim = (ppb / 30 * 30).clamp(-1_000_000, 1_000_000);
            self.trim
        }
    }
    let mut pll = Pll { trim: 0 };
    let mut c = SharedClock::default();
    let error = -50_000.0; // ppb
    let (mut host, mut local) = (0.0f64, 0.0f64);
    let mut rates: Vec<f64> = Vec::new();
    let mut trims = Vec::new();
    for _ in 0..1800 {
        // one second of host time
        let local_rate = 1.0 + (error + pll.trim as f64) * 1e-9;
        host += 1e6;
        local += 1e6 * local_rate;
        rates.push(1.0 / local_rate - 1.0);
        let window = &rates[rates.len().saturating_sub(30)..];
        let seen = window.iter().sum::<f64>() / window.len() as f64;
        let p = point(local as u64, UTC + host as u64, (seen * 1e9) as i32);
        c.steer_with(local as u64, p, Source::Host, &mut pll)
            .unwrap();
        trims.push(pll.trim);
    }
    // the oscillator ends up taking the 50 ppm, the software only the rest
    assert!((pll.trim - 50_000).abs() < 500, "trim {}", pll.trim);
    let (_, trim) = c.rates();
    assert_eq!(trim, pll.trim);
    // no overshoot past 10 %
    let max = trims.iter().max().unwrap();
    assert!(*max < 55_000, "overshoot to {max}");
    // the shared clock stayed on the host's time all along: at the last point
    let s = c.shared(local as u64).unwrap();
    assert!(
        s.abs_diff(UTC + host as u64) < 50,
        "{}",
        s as i64 - (UTC + host as u64) as i64
    );
}

#[test]
fn no_oscillator_keeps_everything_in_software() {
    let mut c = SharedClock::default();
    c.steer_with(0, point(0, UTC, 5_000), Source::Host, &mut NoOscillator)
        .unwrap();
    assert_eq!(c.rates(), (5_000, 0));
}

#[test]
fn the_source_list_decides() {
    let irig = Source::Line(Line::IrigB);
    let pps = Source::Line(Line::Pps);
    let mut c = SharedClock::default();
    assert_eq!(c.sources(), None);
    // never let a PC steer me
    c.set_sources(Some(Sources::new(&[irig]).unwrap()));
    assert_eq!(c.sources(), Some(&[irig][..]));
    assert_eq!(
        c.steer(0, point(0, UTC, 0), Source::Host),
        Ok(Steered::Ignored(None))
    );
    assert_eq!(c.shared(0), None);
    assert_eq!(c.steer(0, point(0, UTC, 0), irig), Ok(Steered::Stepped));
    assert_eq!(
        c.steer(10, point(10, UTC + 10, 0), Source::Host),
        Ok(Steered::Ignored(Some(irig)))
    );
    // PPS before IRIG-B: a PPS point wins over the locked IRIG-B line, and IRIG-B is then only noted
    c.set_sources(Some(Sources::new(&[pps, irig, Source::Host]).unwrap()));
    assert_eq!(
        c.steer(20, point(20, UTC + 25, 0), pps),
        Ok(Steered::Slewing(5))
    );
    assert_eq!(
        c.steer(30, point(30, UTC + 30, 0), irig),
        Ok(Steered::Ignored(Some(pps)))
    );
    // the host before the line: followed while the line is locked
    c.set_sources(Some(Sources::new(&[Source::Host, pps]).unwrap()));
    assert!(matches!(
        c.steer(40, point(40, UTC + 45, 0), Source::Host),
        Ok(Steered::Slewing(_))
    ));
    assert_eq!(c.source(), Some(Source::Host));
    // a list that leaves the followed source out: holdover on it
    c.set_sources(Some(Sources::new(&[pps]).unwrap()));
    assert_eq!(c.state(50), State::Holdover);
    // back to the fixed ranking
    c.set_sources(None);
    assert_eq!(c.state(50), State::Locked);
    assert_eq!(Sources::new(&[irig, irig]), Err(SourcesError::Twice));
    assert_eq!(
        Sources::new(&[irig, pps, Source::Host, irig, pps]),
        Err(SourcesError::TooMany)
    );
}

/// A 1 ms lock tolerance, for round numbers.
const ONE_MS: Config = Config {
    lock_within_us: 1_000,
    ..Config::DEFAULT
};

#[test]
fn locked_until_a_point_finds_it_twice_the_setting_off() {
    let mut c = SharedClock::new(ONE_MS);
    c.steer(0, point(0, UTC, 0), Source::Host).unwrap();
    // on the point, within its 100 µs error
    assert_eq!(c.state(0), State::Locked);
    // 1.5 ms off: more than the 1 ms setting, but within twice it: stays locked, slewed at the locked 1 %
    c.steer(
        1_000_000,
        point(1_000_000, UTC + 1_001_500, 0),
        Source::Host,
    )
    .unwrap();
    assert_eq!(c.state(1_000_000), State::Locked);
    let t = ticks(&mut c, 1_000_000, 20);
    assert!(
        t.windows(2)
            .all(|w| (10_000..=10_100).contains(&(w[1] - w[0])))
    );
    // 2.5 ms off: acquiring again until the slew is in
    c.steer(
        2_000_000,
        point(2_000_000, UTC + 2_004_000, 0),
        Source::Host,
    )
    .unwrap();
    assert_eq!(c.state(2_000_000), State::Acquiring);
    assert_eq!(c.slew_left_at(2_000_000), 2_500);
    // 5 % of 40 ms: 2 ms in, 0.5 ms + 100 µs left: within the setting again
    assert!((500..=501).contains(&c.slew_left_at(2_040_000)));
    assert_eq!(c.state(2_040_000), State::Locked);
    // from acquiring, a point needs to be within the setting itself: 1.5 ms is not
    let mut c = SharedClock::new(ONE_MS);
    c.steer(0, point(0, UTC, 0), Source::Host).unwrap();
    c.steer(
        1_000_000,
        point(1_000_000, UTC + 1_005_000, 0),
        Source::Host,
    )
    .unwrap();
    c.steer(
        1_010_000,
        point(1_010_000, UTC + 1_016_500, 0),
        Source::Host,
    )
    .unwrap();
    assert_eq!(c.state(1_010_000), State::Acquiring);
}

#[test]
fn a_point_with_a_large_error_never_locks() {
    let mut c = SharedClock::new(ONE_MS);
    let p = Steer {
        error_us: 1_500,
        ..point(0, UTC, 0)
    };
    c.steer(0, p, Source::Host).unwrap();
    assert_eq!(c.state(0), State::Acquiring);
    assert_eq!(c.state(3_000_001), State::Holdover);
}

/// The first lock after a long holdover (P2528#21oct-1838): Donguru took 25 s to slew out 250 ms at 1 %; at the
/// acquiring 5 % it is in within 5 s, and shared time never jumps or goes back.
#[test]
fn a_large_offset_is_slewed_fast_while_acquiring() {
    let mut c = SharedClock::default();
    c.steer(0, point(0, UTC, 0), Source::Host).unwrap();
    // a point every 100 ms (the host steers at 10 Hz while acquiring), 250 ms ahead of the clock
    let mut locked_at = None;
    let mut last = c.shared(0).unwrap();
    for i in 1..=100u64 {
        let now = i * 100_000;
        for tick in 1..=10 {
            let s = c.shared(now - 100_000 + tick * 10_000).unwrap();
            // 10 ms a tick plus at most 5 %
            assert!((10_000..=10_500).contains(&(s - last)), "{}", s - last);
            last = s;
        }
        c.steer(now, point(now, UTC + now + 250_000, 0), Source::Host)
            .unwrap();
        if locked_at.is_none() && c.state(now) == State::Locked {
            locked_at = Some(now);
        }
    }
    let locked_at = locked_at.expect("locked");
    assert!((5_000_000..5_200_000).contains(&locked_at), "{locked_at}");
}
