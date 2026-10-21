//! The async driver: [Indication] in a `static`, changed from anywhere, shown by [Indication::run] in a task.

use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::signal::Signal;
use embassy_time::{Duration, Ticker};

use crate::{Leds, Mode, TICK_MS};

type Sig<T> = Signal<CriticalSectionRawMutex, T>;

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
    modes: [Sig<Mode>; N],
    visible: [Sig<bool>; N],
    colours: [Sig<[u8; 3]>; N],
    peak: Sig<u8>,
    identify: Sig<u32>,
    /// Wakes the task when it is idle (no animation running).
    wake: Sig<()>,
}

impl<const N: usize> Indication<N> {
    pub const fn new() -> Self {
        Self {
            modes: [const { Signal::new() }; N],
            visible: [const { Signal::new() }; N],
            colours: [const { Signal::new() }; N],
            peak: Signal::new(),
            identify: Signal::new(),
            wake: Signal::new(),
        }
    }

    /// Change the mode of one LED; breath and blink restart from dark.
    pub fn set(&self, led: usize, mode: Mode) {
        self.modes[led].signal(mode);
        self.wake.signal(());
    }

    /// Show or hide one LED (default shown); its animation keeps running hidden.
    pub fn set_visible(&self, led: usize, visible: bool) {
        self.visible[led].signal(visible);
        self.wake.signal(());
    }

    /// Colour of an RGB LED (default white), see [Leds::set_colour].
    pub fn set_colour(&self, led: usize, rgb: [u8; 3]) {
        self.colours[led].signal(rgb);
        self.wake.signal(());
    }

    /// Peak brightness of all LEDs (default [DEFAULT_PEAK](crate::DEFAULT_PEAK)), without restarting their
    /// animations.
    pub fn set_peak(&self, peak: u8) {
        self.peak.signal(peak);
        self.wake.signal(());
    }

    /// Blink every LED at full brightness for `ms` (restarts a running identify, 0 ends it), then show the
    /// animations again.
    pub fn identify(&self, ms: u32) {
        self.identify.signal(ms);
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
        for i in 0..N {
            if let Some(mode) = self.modes[i].try_take() {
                state.set(i, mode);
            }
            if let Some(v) = self.visible[i].try_take() {
                state.set_visible(i, v);
            }
            if let Some(c) = self.colours[i].try_take() {
                state.set_colour(i, c);
            }
        }
        if let Some(p) = self.peak.try_take() {
            state.set_peak(p);
        }
        if let Some(ms) = self.identify.try_take() {
            state.identify(ms);
        }
    }
}

impl<const N: usize> Default for Indication<N> {
    fn default() -> Self {
        Self::new()
    }
}
