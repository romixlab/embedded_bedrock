//! The async driver: [Indication] in a `static`, changed from anywhere, shown by [Indication::run] in a task.

use core::cell::Cell;

use embassy_sync::blocking_mutex::Mutex;
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::signal::Signal;
use embassy_time::{Duration, Ticker};

use crate::{Leds, Mode, TICK_MS};

/// N LEDs under one indication, as a `static` the firmware changes from any task or interrupt; [run](Self::run)
/// in the LED task shows it. Changes take effect on the next tick; with nothing animated the task sleeps until a
/// change.
///
/// ```ignore
/// use bedrock_led::{Channel, Indication, Mode, Pwm};
///
/// pub static LEDS: Indication<2> = Indication::new();
///
/// #[embassy_executor::task]
/// async fn led_task(mut power: Pwm<SimplePwmChannel<'static, TIM14>>, mut rgb: [Pwm<SimplePwmChannel<'static, TIM3>>; 3]) {
///     LEDS.run([Mode::Breath { period_ms: 3000 }, Mode::On], |leds| {
///         power.set_level(leds.level(0));
///         for (ch, level) in rgb.iter_mut().zip(leds.rgb(1)) {
///             ch.set_level(level);
///         }
///     })
///     .await
/// }
/// ```
pub struct Indication<const N: usize> {
    /// What changed since the last tick, one critical section for all of it. All zero when nothing changed, so
    /// the `static` sits in `.bss`.
    pending: Mutex<CriticalSectionRawMutex, Pending<N>>,
    /// Wakes the task when it is idle (no animation running).
    wake: Signal<CriticalSectionRawMutex, ()>,
}

/// Changes waiting for the next tick: a value per kind and LED, valid where its bit in the mask is set (a later
/// change of the same kind overwrites an earlier one).
struct Pending<const N: usize> {
    modes: [Cell<Mode>; N],
    visible: [Cell<bool>; N],
    colours: [Cell<[u8; 3]>; N],
    peak: Cell<u8>,
    identify_ms: Cell<u32>,
    /// One bit per LED.
    modes_set: Cell<u32>,
    visible_set: Cell<u32>,
    colours_set: Cell<u32>,
    peak_set: Cell<bool>,
    identify_set: Cell<bool>,
}

impl<const N: usize> Indication<N> {
    pub const fn new() -> Self {
        const { assert!(N <= 32, "one bit per LED in a u32") };
        Self {
            pending: Mutex::const_new(
                CriticalSectionRawMutex::new(),
                Pending {
                    modes: [const { Cell::new(Mode::Off) }; N],
                    visible: [const { Cell::new(false) }; N],
                    colours: [const { Cell::new([0; 3]) }; N],
                    peak: Cell::new(0),
                    identify_ms: Cell::new(0),
                    modes_set: Cell::new(0),
                    visible_set: Cell::new(0),
                    colours_set: Cell::new(0),
                    peak_set: Cell::new(false),
                    identify_set: Cell::new(false),
                },
            ),
            wake: Signal::new(),
        }
    }

    /// Change the mode of one LED; breath and blink restart from dark.
    pub fn set(&self, led: usize, mode: Mode) {
        self.change(|p| {
            p.modes[led].set(mode);
            p.modes_set.set(p.modes_set.get() | 1 << led);
        });
    }

    /// Show or hide one LED (default shown); its animation keeps running hidden.
    pub fn set_visible(&self, led: usize, visible: bool) {
        self.change(|p| {
            p.visible[led].set(visible);
            p.visible_set.set(p.visible_set.get() | 1 << led);
        });
    }

    /// Colour of an RGB LED (default white), see [Leds::set_colour].
    pub fn set_colour(&self, led: usize, rgb: [u8; 3]) {
        self.change(|p| {
            p.colours[led].set(rgb);
            p.colours_set.set(p.colours_set.get() | 1 << led);
        });
    }

    /// Peak brightness of all LEDs (default [DEFAULT_PEAK](crate::DEFAULT_PEAK)), without restarting their
    /// animations.
    pub fn set_peak(&self, peak: u8) {
        self.change(|p| {
            p.peak.set(peak);
            p.peak_set.set(true);
        });
    }

    /// Blink every LED at full brightness for `ms` (restarts a running identify, 0 ends it), then show the
    /// animations again.
    pub fn identify(&self, ms: u32) {
        self.change(|p| {
            p.identify_ms.set(ms);
            p.identify_set.set(true);
        });
    }

    fn change(&self, f: impl FnOnce(&Pending<N>)) {
        self.pending.lock(f);
        self.wake.signal(());
    }

    /// Runs the indication from `modes`: every [TICK_MS] (or after a change, when idle), `show` writes the levels
    /// of this tick to the channels, [Leds::level] for a mono LED, [Leds::rgb] for an RGB one.
    pub async fn run(&self, modes: [Mode; N], mut show: impl FnMut(&Leds<N>)) -> ! {
        // all off, then the modes one by one: no Animation::new inlined per LED, and the state is built in place
        let mut state = Leds::new([Mode::Off; N]);
        for (i, mode) in modes.into_iter().enumerate() {
            state.set(i, mode);
        }
        let mut ticker = Ticker::every(Duration::from_millis(TICK_MS as u64));
        loop {
            self.take(&mut state);
            show(&state);
            if state.advance() {
                ticker.next().await;
            } else {
                // nothing to animate: sleep until a mode, brightness or identify changes
                self.wake.wait().await;
                ticker.reset();
            }
        }
    }

    /// Applies what changed since the last tick.
    fn take(&self, state: &mut Leds<N>) {
        self.pending.lock(|p| {
            let (modes, visible, colours) = (
                p.modes_set.replace(0),
                p.visible_set.replace(0),
                p.colours_set.replace(0),
            );
            for i in 0..N {
                if modes & 1 << i != 0 {
                    state.set(i, p.modes[i].get());
                }
                if visible & 1 << i != 0 {
                    state.set_visible(i, p.visible[i].get());
                }
                if colours & 1 << i != 0 {
                    state.set_colour(i, p.colours[i].get());
                }
            }
            if p.peak_set.replace(false) {
                state.set_peak(p.peak.get());
            }
            if p.identify_set.replace(false) {
                state.identify(p.identify_ms.get());
            }
        });
    }
}

impl<const N: usize> Default for Indication<N> {
    fn default() -> Self {
        Self::new()
    }
}
