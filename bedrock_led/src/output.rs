//! PWM outputs: [Channel] is what the driver writes, [Pwm] adapts an embedded-hal `SetDutyCycle`.

use embedded_hal::pwm::SetDutyCycle;

use crate::duty;

/// One PWM output driving an LED (or one colour of an RGB LED).
///
/// [Pwm] implements it for any embedded-hal `SetDutyCycle`. A board implements it itself where the HAL has no
/// `SetDutyCycle` for the output (a complementary timer channel) or reports a `max_duty_cycle` that isn't 100 %.
pub trait Channel {
    /// Duty for 100 % on, at most 65536.
    fn max_duty(&self) -> u32;
    /// How long the LED is lit, 0..=[max_duty](Self::max_duty).
    fn set_duty(&mut self, duty: u32);

    /// Shows a perceptual level (0..=65535) with gamma 2 ([duty]).
    fn set_level(&mut self, level: u32) {
        let max = self.max_duty();
        self.set_duty(duty(level, max));
    }
}

/// An embedded-hal PWM output as a [Channel]; active low inverts the duty (LED from the supply to the pin).
///
/// `max_duty_cycle` is read once, here: wrap the output again after changing its frequency. (Some HALs compute it
/// with a fallible conversion, which every tick would pay for.)
pub struct Pwm<P> {
    pwm: P,
    max: u16,
    active_low: bool,
}

impl<P: SetDutyCycle> Pwm<P> {
    /// LED lit while the output is high.
    pub fn new(pwm: P) -> Self {
        Self::with_polarity(pwm, false)
    }

    /// LED lit while the output is low.
    pub fn active_low(pwm: P) -> Self {
        Self::with_polarity(pwm, true)
    }

    fn with_polarity(pwm: P, active_low: bool) -> Self {
        Self {
            max: pwm.max_duty_cycle(),
            pwm,
            active_low,
        }
    }

    /// The wrapped output.
    pub fn inner(&mut self) -> &mut P {
        &mut self.pwm
    }
}

impl<P: SetDutyCycle> Channel for Pwm<P> {
    fn max_duty(&self) -> u32 {
        self.max as u32
    }

    fn set_duty(&mut self, duty: u32) {
        let max = self.max as u32;
        let duty = duty.min(max);
        let duty = if self.active_low { max - duty } else { duty };
        // within max_duty_cycle, the only error SetDutyCycle defines; an LED has nothing to do on others
        let _ = self.pwm.set_duty_cycle(duty as u16);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use core::convert::Infallible;

    struct Fake {
        max: u16,
        duty: u16,
    }

    impl embedded_hal::pwm::ErrorType for Fake {
        type Error = Infallible;
    }

    impl SetDutyCycle for Fake {
        fn max_duty_cycle(&self) -> u16 {
            self.max
        }

        fn set_duty_cycle(&mut self, duty: u16) -> Result<(), Infallible> {
            assert!(duty <= self.max);
            self.duty = duty;
            Ok(())
        }
    }

    #[test]
    fn active_high_and_low() {
        let mut hi = Pwm::new(Fake { max: 1000, duty: 0 });
        hi.set_level(65535);
        assert_eq!(hi.inner().duty, 1000);
        hi.set_level(0);
        assert_eq!(hi.inner().duty, 0);
        let mut lo = Pwm::active_low(Fake { max: 1000, duty: 0 });
        lo.set_level(65535);
        assert_eq!(lo.inner().duty, 0);
        lo.set_level(0);
        assert_eq!(lo.inner().duty, 1000);
        lo.set_level(32768);
        assert_eq!(lo.inner().duty, 750);
        // out of range is clamped, not passed on
        lo.set_duty(5000);
        assert_eq!(lo.inner().duty, 0);
    }
}
