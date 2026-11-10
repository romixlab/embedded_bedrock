//! The one error of exact evaluation.

use core::fmt;

/// An exact step of the tree went past what a `Fraction` holds (u128 numerator and denominator).
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct Overflow;

impl fmt::Display for Overflow {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("exact arithmetic overflowed u128")
    }
}

impl std::error::Error for Overflow {}
