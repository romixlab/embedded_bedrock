//! N LEDs under one indication, HAL-free: what each shows this tick.

use crate::{Animation, DEFAULT_PEAK, Mode, TICK_MS, scale};

/// The indication state of N LEDs: an [Animation] each, which are visible, one peak brightness for all, a colour
/// for RGB LEDs, and a running identify. The levels are perceptual (0..=65535), ready for [duty](crate::duty).
#[derive(Clone, Debug)]
pub struct Leds<const N: usize> {
    leds: [Animation; N],
    visible: [bool; N],
    colours: [[u8; 3]; N],
    peak: u8,
    /// A running identify: ms left and its blink.
    identify: Option<(u32, Animation)>,
}

impl<const N: usize> Leds<N> {
    /// Starts `modes`, every LED visible and white, peak [DEFAULT_PEAK], no identify.
    pub fn new(modes: [Mode; N]) -> Self {
        Self {
            leds: modes.map(Animation::new),
            visible: [true; N],
            colours: [[255; 3]; N],
            peak: DEFAULT_PEAK,
            identify: None,
        }
    }

    /// Change the mode of one LED: breath and blink restart from dark.
    #[inline(never)] // one copy of Animation::new instead of one per LED, for flash on the M0+
    pub fn set(&mut self, led: usize, mode: Mode) {
        self.leds[led] = Animation::new(mode);
    }

    /// Show or hide one LED; its animation keeps running hidden.
    pub fn set_visible(&mut self, led: usize, visible: bool) {
        self.visible[led] = visible;
    }

    /// Colour of an RGB LED, perceptual per channel (255 = the channel at the peak brightness). Mono LEDs ignore it.
    pub fn set_colour(&mut self, led: usize, rgb: [u8; 3]) {
        self.colours[led] = rgb;
    }

    /// Peak brightness of all LEDs, perceptual (0 = off, 255 = full duty), without restarting their animations.
    pub fn set_peak(&mut self, peak: u8) {
        self.peak = peak;
    }

    /// Blink every LED at full brightness for `ms` (restarts a running identify, 0 ends it), then show the
    /// animations again.
    pub fn identify(&mut self, ms: u32) {
        self.identify = (ms > 0).then(|| (ms, Animation::identify()));
    }

    /// Level of a mono LED this tick: the identify blink at full brightness, or the animation at the peak when
    /// visible.
    pub fn level(&self, led: usize) -> u32 {
        // one call of Animation::level: it inlines, and flash is tight on the M0+
        let (animation, peak) = match &self.identify {
            Some((_, blink)) => (blink, 255),
            None if self.visible[led] => (&self.leds[led], self.peak),
            None => return 0,
        };
        scale(animation.level(), peak)
    }

    /// Levels of an RGB LED this tick: [level](Self::level) per channel scaled by the colour; identify blinks
    /// white.
    pub fn rgb(&self, led: usize) -> [u32; 3] {
        let level = self.level(led);
        match self.identify {
            Some(_) => [level; 3],
            None => self.colours[led].map(|c| scale(level, c)),
        }
    }

    /// One [TICK_MS] on: every animation, and the identify, which ends after its time. Returns whether the next
    /// tick is needed: [animated](Self::animated) before this step, so the tick after an identify still runs and
    /// shows the animations again before the driver sleeps.
    pub fn advance(&mut self) -> bool {
        let animated = self.animated();
        for led in &mut self.leds {
            led.advance();
        }
        if let Some((left, blink)) = &mut self.identify {
            blink.advance();
            *left = left.saturating_sub(TICK_MS);
            if *left == 0 {
                self.identify = None;
            }
        }
        animated
    }

    /// Whether the levels change over time (the driver can sleep until a change otherwise).
    pub fn animated(&self) -> bool {
        self.identify.is_some() || self.leds.iter().any(Animation::animated)
    }

    /// Whether an identify runs.
    pub fn identifying(&self) -> bool {
        self.identify.is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{FULL, Mode, duty};

    const BREATH: Mode = Mode::Breath { period_ms: 1000 };

    /// Levels of LED `led` over `ticks`, as the driver shows them: level, then advance.
    fn run(leds: &mut Leds<2>, led: usize, ticks: usize) -> Vec<u32> {
        (0..ticks)
            .map(|_| {
                let l = leds.level(led);
                leds.advance();
                l
            })
            .collect()
    }

    #[test]
    fn peak_and_visibility() {
        let mut leds = Leds::new([Mode::On, Mode::On]);
        assert_eq!(leds.level(0), scale(FULL, DEFAULT_PEAK));
        leds.set_peak(255);
        assert_eq!(leds.level(1), FULL);
        leds.set_visible(1, false);
        assert_eq!(leds.level(1), 0);
        assert_eq!(leds.level(0), FULL);
        assert!(!leds.animated());
    }

    #[test]
    fn hidden_animation_keeps_running() {
        let mut leds = Leds::new([BREATH, BREATH]);
        leds.set_visible(1, false);
        run(&mut leds, 1, 50);
        leds.set_visible(1, true);
        // half a period on: at the top of the breath, as LED 0
        assert_eq!(leds.level(1), leds.level(0));
        assert!(leds.level(1) > scale(65000, DEFAULT_PEAK));
    }

    #[test]
    fn peak_change_keeps_the_phase() {
        let mut leds = Leds::new([BREATH, Mode::Off]);
        let a = run(&mut leds, 0, 25);
        leds.set_peak(255);
        let b = leds.level(0);
        assert!(
            b > *a.last().unwrap() * 255 / DEFAULT_PEAK as u32 / 2,
            "{b}"
        );
    }

    #[test]
    fn identify_overrides_then_ends() {
        let mut leds = Leds::new([Mode::Off, Mode::On]);
        leds.set_visible(1, false);
        leds.set_peak(8);
        leds.identify(300);
        assert!(leds.animated() && leds.identifying());
        let l0 = run(&mut leds, 0, 30);
        // full brightness whatever the peak, hidden or off LEDs too: on at 100 ms, off by 250 ms
        assert!(l0[10] >= 65533);
        assert_eq!(l0[25], 0);
        // 300 ms = 30 ticks, then back; the tick after it still runs to show that
        assert!(!leds.identifying());
        let mut again = leds.clone();
        again.identify(10);
        assert!(again.advance(), "the tick after the identify");
        assert!(!again.advance());
        assert!(!leds.animated());
        assert_eq!(leds.level(0), 0);
        assert_eq!(leds.level(1), 0);
        leds.set_visible(1, true);
        assert_eq!(leds.level(1), scale(FULL, 8));
    }

    #[test]
    fn identify_restart_and_cancel() {
        let mut leds = Leds::new([Mode::Off, Mode::Off]);
        leds.identify(1000);
        run(&mut leds, 0, 12);
        // restart: from dark again, the full time
        leds.identify(1000);
        assert_eq!(leds.level(0), 0);
        run(&mut leds, 0, 99);
        assert!(leds.identifying());
        leds.advance();
        assert!(!leds.identifying());
        leds.identify(1000);
        leds.identify(0);
        assert!(!leds.identifying());
        // a time that is not a multiple of the tick rounds up
        leds.identify(15);
        leds.advance();
        assert!(leds.identifying());
        leds.advance();
        assert!(!leds.identifying());
    }

    #[test]
    fn rgb_colour_is_per_channel_peak() {
        let mut leds = Leds::new([Mode::On, Mode::Off]);
        leds.set_peak(255);
        assert_eq!(leds.rgb(0), [FULL; 3]);
        leds.set_colour(0, [255, 128, 0]);
        assert_eq!(leds.rgb(0), [FULL, scale(FULL, 128), 0]);
        // the peak scales every channel
        leds.set_peak(128);
        assert_eq!(leds.rgb(0)[0], scale(FULL, 128));
        assert_eq!(leds.rgb(0)[1], scale(scale(FULL, 128), 128));
        // half perceptual green is a quarter of the duty
        leds.set_peak(255);
        assert!(duty(leds.rgb(0)[1], 65536).abs_diff(16384) < 300);
        // identify blinks white
        leds.identify(1000);
        for _ in 0..10 {
            leds.advance();
        }
        assert_eq!(leds.rgb(0), [leds.level(0); 3]);
        assert!(leds.level(0) >= 65533);
        // a hidden RGB LED is dark
        leds.identify(0);
        leds.set_visible(0, false);
        assert_eq!(leds.rgb(0), [0; 3]);
    }

    #[test]
    fn mono_level_ignores_colour() {
        let mut leds = Leds::new([Mode::On, Mode::Off]);
        leds.set_colour(0, [0, 0, 0]);
        assert_eq!(leds.level(0), scale(FULL, DEFAULT_PEAK));
    }
}
