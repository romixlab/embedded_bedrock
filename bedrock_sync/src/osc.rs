//! Hardware steering: an oscillator the firmware can trim (a fractional PLL), see [Oscillator].

/// An oscillator the local clock runs from, trimmable in ppb (a fractional PLL's FRACN on the fly).
///
/// With one, [SharedClock::steer_with](crate::SharedClock::steer_with) moves part of each steer point's rate to the
/// oscillator, so after a while the local clock (and embassy-time, and every peripheral clocked from it) runs at the
/// shared rate and the software correction keeps only the rest. Coarse trims that wobble other peripherals (an RC
/// oscillator's trim register: UART baud and PWM jump) are not a good backend; leave the software clock alone there.
pub trait Oscillator {
    /// Sets the trim to `ppb` from nominal (positive: faster) and returns the trim actually set, quantised to the
    /// hardware's step and clamped to its range.
    fn set_trim_ppb(&mut self, ppb: i32) -> i32;

    /// How much to move the trim for a steer point with `rate_ppb`, 0 for none. The default moves 1/64 of the rate
    /// when it is more than 100 ppb off: the host fits over a window of about 30 s, so its rate lags a trim change
    /// by ~15 s, and a larger share per point (once a second) would overshoot and oscillate.
    fn trim_step_ppb(&mut self, rate_ppb: i32) -> i32 {
        if rate_ppb.unsigned_abs() > 100 {
            // rate_ppb > 0: shared runs faster than local, so the oscillator should run faster
            rate_ppb / 64
        } else {
            0
        }
    }
}

/// No hardware steering: the whole rate stays in software.
#[derive(Copy, Clone, Debug, Default)]
pub struct NoOscillator;

impl Oscillator for NoOscillator {
    fn set_trim_ppb(&mut self, _ppb: i32) -> i32 {
        0
    }

    fn trim_step_ppb(&mut self, _rate_ppb: i32) -> i32 {
        0
    }
}
