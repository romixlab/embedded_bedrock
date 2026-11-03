//! Blink codes (DEV-18): a number on the status LED from a firmware that won't start, the same pattern on every
//! board. See `book/blink_codes.md` for the table a person reads.
//!
//! A code is one long flash, then that many short flashes, then a pause; repeated for [DURATION_MS] at least.
//! Red ([COLOUR]) on an RGB LED, plain on a mono one. It runs before anything else: [blink] is blocking and
//! HAL-free, the board gives it a closure that drives the LED pin as a plain GPIO output and one that waits.
//!
//! ```
//! use bedrock_led::code::{blink, Code};
//! let log = std::cell::RefCell::new(Vec::new());
//! let on = std::cell::Cell::new(false);
//! blink(Code::UnknownRework, |o| on.set(o), |ms| log.borrow_mut().push((on.get(), ms)));
//! let log = log.into_inner();
//! // a long flash, one short flash, the pause, again and again for about 10 s
//! assert_eq!(&log[..4], &[(true, 1000), (false, 400), (true, 250), (false, 1500)]);
//! ```

use crate::look::colour;

/// Why a firmware won't start. The number is what the LED shows; a new reason takes the next one.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
#[repr(u8)]
pub enum Code {
    /// The board record has a rework applied that this image doesn't know, and no allowance for this image covers
    /// it (wire_weaver STD-101): the board may be damaged by any init, so nothing is set up. Flash an image that
    /// knows the rework, or allow it with `ww board rework allow` while the board is in its ROM bootloader.
    UnknownRework = 1,
    /// The board record can't be read (a newer format, damaged, a store that doesn't fit): the firmware can't tell
    /// what the board is, so it treats it as code 1. `ww info` in the ROM bootloader says what it found.
    RecordUnreadable = 2,
}

impl Code {
    /// Short flashes after the long one.
    pub const fn flashes(self) -> u8 {
        self as u8
    }
}

/// The colour of a blink code on an RGB LED.
pub const COLOUR: [u8; 3] = colour::FAULT;
/// The long flash that starts a code.
pub const LONG_MS: u32 = 1000;
/// A short flash.
pub const SHORT_ON_MS: u32 = 250;
/// Dark between flashes, also after the long one.
pub const GAP_MS: u32 = 400;
/// Dark after the last short flash, before the code starts again.
pub const PAUSE_MS: u32 = 1500;
/// A code is blinked for at least this long, in whole repetitions; then the firmware goes on (into the ROM
/// bootloader).
pub const DURATION_MS: u32 = 10_000;

/// One repetition of `code`: `(on, ms)` steps, the LED on or off for that long.
pub fn steps(code: Code) -> impl Iterator<Item = (bool, u32)> {
    let n = code.flashes() as usize;
    [(true, LONG_MS), (false, GAP_MS)]
        .into_iter()
        .chain((0..n).flat_map(move |i| {
            let dark = if i + 1 == n { PAUSE_MS } else { GAP_MS };
            [(true, SHORT_ON_MS), (false, dark)]
        }))
}

/// The length of one repetition of `code`.
pub fn period_ms(code: Code) -> u32 {
    steps(code).map(|(_, ms)| ms).sum()
}

/// Blink `code` for [DURATION_MS] (whole repetitions), then leave the LED off. `set_led(true)` lights it,
/// `delay_ms` waits (a busy loop on the boot clock is fine: the timing is for a person).
pub fn blink(code: Code, mut set_led: impl FnMut(bool), mut delay_ms: impl FnMut(u32)) {
    let mut elapsed = 0;
    while elapsed < DURATION_MS {
        for (on, ms) in steps(code) {
            set_led(on);
            delay_ms(ms);
        }
        elapsed += period_ms(code);
    }
    set_led(false);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pattern() {
        let one: Vec<_> = steps(Code::UnknownRework).collect();
        assert_eq!(
            one,
            [(true, 1000), (false, 400), (true, 250), (false, 1500)]
        );
        let two: Vec<_> = steps(Code::RecordUnreadable).collect();
        assert_eq!(
            two,
            [
                (true, 1000),
                (false, 400),
                (true, 250),
                (false, 400),
                (true, 250),
                (false, 1500)
            ]
        );
        assert_eq!(period_ms(Code::UnknownRework), 3150);
    }

    #[test]
    fn blinks_whole_repetitions_for_the_duration() {
        let (mut total, mut last) = (0, None);
        let mut ons = 0;
        blink(
            Code::UnknownRework,
            |on| {
                ons += on as u32;
                last = Some(on)
            },
            |ms| total += ms,
        );
        assert_eq!(last, Some(false));
        // 4 repetitions of 3.15 s: the first count past 10 s
        assert_eq!(total, 4 * 3150);
        assert_eq!(ons, 4 * 2);
    }
}
