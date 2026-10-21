//! The async side: [Sync], a [SharedClock] in a `static` on embassy-time's `Instant`.

use core::cell::RefCell;

use embassy_sync::blocking_mutex::Mutex;
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_time::{Duration, Instant, Timer};

use crate::{Config, Oscillator, SharedClock, Source, State, Steer, SteerError, Steered};

/// Shared time now, see [Sync::shared_now].
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub struct SharedNow {
    /// µs since the Unix epoch, UTC.
    pub us: u64,
    /// [SharedClock::generation]: changes when shared time stepped or switched source.
    pub generation: u32,
}

/// The board's shared clock in a `static`, local time from embassy-time's `Instant` (µs). Steered from the `Clock`
/// server's handler, read by the LED task every tick, waited on for scheduled actions.
///
/// ```ignore
/// pub static SYNC: bedrock_sync::Sync = bedrock_sync::Sync::new(bedrock_sync::Config::DEFAULT);
///
/// // in the LED task: aligned to UTC once steered
/// let shared = SYNC.shared_now();
///
/// // something at the same shared time on every board
/// SYNC.wait_until_shared(t).await?;
/// ```
pub struct Sync {
    clock: Mutex<CriticalSectionRawMutex, RefCell<SharedClock>>,
}

/// [Sync::wait_until_shared] without a shared clock.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub struct NotSynced;

impl Sync {
    pub const fn new(config: Config) -> Self {
        Self {
            clock: Mutex::const_new(
                CriticalSectionRawMutex::new(),
                RefCell::new(SharedClock::new(config)),
            ),
        }
    }

    /// Local time, µs.
    pub fn local_us() -> u64 {
        Instant::now().as_micros()
    }

    /// Runs `f` on the clock with the local time now, in one critical section.
    pub fn with<R>(&self, f: impl FnOnce(&mut SharedClock, u64) -> R) -> R {
        self.clock
            .lock(|c| f(&mut c.borrow_mut(), Self::local_us()))
    }

    /// Takes a steer point, see [SharedClock::steer].
    pub fn steer(&self, point: Steer, source: Source) -> Result<Steered, SteerError> {
        self.with(|c, now| c.steer(now, point, source))
    }

    /// Takes a steer point and trims `osc`, see [SharedClock::steer_with].
    pub fn steer_with(
        &self,
        point: Steer,
        source: Source,
        osc: &mut impl Oscillator,
    ) -> Result<Steered, SteerError> {
        self.with(|c, now| c.steer_with(now, point, source, osc))
    }

    /// Shared time now, `None` before the first steer point.
    pub fn shared_now(&self) -> Option<SharedNow> {
        self.with(|c, now| {
            c.shared(now).map(|us| SharedNow {
                us,
                generation: c.generation(),
            })
        })
    }

    pub fn state(&self) -> State {
        self.with(|c, now| c.state(now))
    }

    /// Waits until shared time reaches `shared_us` (UTC µs), to the timer's resolution: the Klipper part, a board
    /// does something at the same shared time as the others. Checks again at least once a second, so a steer point
    /// that moves the moment is followed. `NotSynced` before the first steer point.
    pub async fn wait_until_shared(&self, shared_us: u64) -> Result<(), NotSynced> {
        loop {
            let (now, local) = self.with(|c, now| (c.shared(now), c.local_at(shared_us)));
            let (Some(now), Some(local)) = (now, local) else {
                return Err(NotSynced);
            };
            if now >= shared_us {
                return Ok(());
            }
            let latest = Instant::now() + Duration::from_secs(1);
            Timer::at(Instant::from_micros(local).min(latest)).await;
        }
    }
}
