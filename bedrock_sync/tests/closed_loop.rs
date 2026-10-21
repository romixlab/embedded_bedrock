//! The host's fit (wire_weaver's `ww_clock_sync::Estimator`) steering a [SharedClock] over a simulated USB link, in
//! simulated time: how long a board takes from its first point to `Locked` and within the lock tolerance of the
//! host's UTC, at the host's acquiring pace (10 Hz) and then the normal one (1 Hz), and that shared time never jumps
//! on the way.

use bedrock_sync::{Config, SharedClock, Source, State, Steer};
use ww_clock_sync::{Estimator, EstimatorConfig, Sample};

const UTC: u64 = 1_761_000_000_000_000;

/// A board on the host's USB.
struct Board {
    /// Local clock rate error, ppm (Donguru's HSI is ~ -2000).
    ppm: f64,
    /// Local time at host time 0, µs.
    start_us: f64,
    /// One-way delay, µs: at least `min`, plus up to `jitter` (deterministic, xorshift).
    min_us: u64,
    jitter_us: u64,
}

struct Rng(u64);

impl Rng {
    fn below(&mut self, max: u64) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        if max == 0 { 0 } else { self.0 % max }
    }
}

impl Board {
    fn local_us(&self, host_us: u64) -> u64 {
        (self.start_us + host_us as f64 * (1.0 + self.ppm * 1e-6)) as u64
    }
}

struct Run {
    /// Host time from the first steer point to `Locked` with the residual within the tolerance, for good.
    converged_after_us: u64,
    /// Largest |residual| after that, µs.
    worst_after_us: u64,
    /// Largest step of shared time over a 10 ms LED tick, µs.
    max_tick_us: u64,
}

/// `secs` of host time: polls, fits, steers at the acquiring pace until the board says `Locked`. With `ahead_us`
/// the board starts steered already, by an earlier host, that far ahead of this host's UTC (holdover after `ww sync`
/// ran on another PC, or before a restart of it).
fn run(board: &Board, secs: u64, ahead_us: Option<u64>) -> Run {
    let config = Config::DEFAULT;
    let tolerance = config.lock_within_us as i64;
    let mut clock = SharedClock::new(config);
    if let Some(ahead) = ahead_us {
        let rate_ppb = (1e9 / (1.0 + board.ppm * 1e-6) - 1e9) as i32;
        let earlier = Steer {
            local_us: board.local_us(0),
            shared_us: UTC + ahead,
            rate_ppb,
            error_us: 100,
            steerer: 0,
        };
        clock
            .steer(board.local_us(0), earlier, Source::Host)
            .unwrap();
    }
    let mut est = Estimator::new(EstimatorConfig::default());
    let mut rng = Rng(0x5eed);
    let (mut next_poll, mut next_steer) = (0u64, 0u64);
    let mut first_point = None;
    let mut converged_at: Option<u64> = None;
    let mut worst_after = 0;
    let mut last_shared: Option<u64> = None;
    let mut max_tick = 0;
    let mut in_flight = None;
    for host in (0..secs * 1_000_000).step_by(1_000) {
        let local = board.local_us(host);
        let locked = clock.state(local) == State::Locked;
        let (poll, steer) = if locked {
            (200_000, 1_000_000)
        } else {
            (100_000, 100_000)
        };
        if host >= next_poll {
            next_poll = host + poll;
            let up = board.min_us + rng.below(board.jitter_us);
            let down = board.min_us + rng.below(board.jitter_us);
            est.add(Sample {
                sent: host * 1000,
                device: board.local_us(host + up) * 1000,
                received: (host + up + down) * 1000,
            });
        }
        if host >= next_steer
            && let Some(fit) = est.fit()
        {
            next_steer = host + steer;
            // the point at the device's present by the fit, arriving one delay later
            let at = fit.local_at(host as f64 * 1000.0).max(0.0) as u64;
            let point = Steer {
                local_us: at / 1000,
                shared_us: UTC + (fit.shared_at(at / 1000 * 1000) / 1000.0) as u64,
                rate_ppb: fit.rate_ppb(),
                error_us: fit.error_us(),
                steerer: 0,
            };
            in_flight = Some((host + board.min_us + rng.below(board.jitter_us), point));
            first_point.get_or_insert(host);
        }
        if let Some((arrive, point)) = in_flight
            && host >= arrive
        {
            in_flight = None;
            clock.steer(local, point, Source::Host).unwrap();
        }
        if host % 10_000 == 0
            && let Some(s) = clock.shared(local)
        {
            if let Some(l) = last_shared {
                assert!(s >= l, "shared time went back at {host} µs");
                max_tick = max_tick.max(s - l);
            }
            last_shared = Some(s);
            let residual = s as i64 - (UTC + host) as i64;
            let within = residual.abs() <= tolerance && clock.state(local) == State::Locked;
            match (within, converged_at) {
                (true, None) => converged_at = Some(host),
                (false, Some(_)) => {
                    converged_at = None;
                    worst_after = 0;
                }
                (true, Some(_)) => worst_after = worst_after.max(residual.unsigned_abs()),
                _ => {}
            }
        }
    }
    Run {
        converged_after_us: converged_at.expect("converged") - first_point.unwrap(),
        worst_after_us: worst_after,
        max_tick_us: max_tick,
    }
}

#[test]
fn donguru_converges() {
    // HSI 2000 ppm slow, full-speed USB: ~1.4 ms round trips with up to 0.6 ms more each way
    let r = run(
        &Board {
            ppm: -2000.0,
            start_us: 5_000_000.0,
            min_us: 700,
            jitter_us: 600,
        },
        120,
        None,
    );
    println!(
        "Donguru: within {} µs after {:.1} s, worst {} µs, largest tick {} µs",
        Config::DEFAULT.lock_within_us,
        r.converged_after_us as f64 / 1e6,
        r.worst_after_us,
        r.max_tick_us
    );
    assert!(r.converged_after_us < 30_000_000);
    // 10 ms ticks, at most 5 % slew plus the rate
    assert!(r.max_tick_us <= 10_600, "{}", r.max_tick_us);
}

#[test]
fn b135_converges() {
    // a crystal 5 ppm fast, high-speed USB: ~150 µs round trips
    let r = run(
        &Board {
            ppm: 5.0,
            start_us: 123_000_000.0,
            min_us: 70,
            jitter_us: 60,
        },
        60,
        None,
    );
    println!(
        "B135: within {} µs after {:.1} s, worst {} µs, largest tick {} µs",
        Config::DEFAULT.lock_within_us,
        r.converged_after_us as f64 / 1e6,
        r.worst_after_us,
        r.max_tick_us
    );
    assert!(r.converged_after_us < 5_000_000);
    assert!(r.worst_after_us < 200);
}

#[test]
fn a_board_250_ms_off_slews_in_within_seconds() {
    let donguru = Board {
        ppm: -2000.0,
        start_us: 5_000_000.0,
        min_us: 700,
        jitter_us: 600,
    };
    let r = run(&donguru, 60, Some(250_000));
    println!(
        "Donguru 250 ms off: within {} µs after {:.1} s, worst {} µs, largest tick {} µs",
        Config::DEFAULT.lock_within_us,
        r.converged_after_us as f64 / 1e6,
        r.worst_after_us,
        r.max_tick_us
    );
    // 1 % took 25 s (P2528#21oct-1838); 5 % takes 5 s, plus the fit's first second
    assert!(r.converged_after_us < 8_000_000, "{}", r.converged_after_us);
    assert!(r.max_tick_us <= 10_600, "{}", r.max_tick_us);
}
