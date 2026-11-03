//! An exact non-negative rational number, num/den in u128, always reduced.
//!
//! Every clock in the tree is one of these: an STM32H7 PLL output is ref·(N + FRACN/8192)/(M·P), which is not an
//! integer and, with a DIVM or DIVP of 3 or 5, not a binary fraction either. Arithmetic is checked: an overflow is an
//! error, never a rounded value. Local until exact_units has a runtime exact rational (exact_units NUM-11).

use core::cmp::Ordering;
use core::fmt;

/// num/den, reduced, den > 0.
#[derive(Copy, Clone, PartialEq, Eq, Hash)]
pub struct Ratio {
    num: u128,
    den: u128,
}

/// An arithmetic step overflowed u128.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct Overflow;

impl fmt::Display for Overflow {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("exact arithmetic overflowed u128")
    }
}

impl std::error::Error for Overflow {}

const fn gcd(mut a: u128, mut b: u128) -> u128 {
    while b != 0 {
        let t = a % b;
        a = b;
        b = t;
    }
    a
}

impl Ratio {
    pub const ZERO: Ratio = Ratio { num: 0, den: 1 };

    /// `num / den`, reduced. Panics on `den == 0` (a programming error, never data).
    pub const fn new(num: u128, den: u128) -> Ratio {
        assert!(den != 0, "Ratio with a zero denominator");
        let g = gcd(num, den);
        if g == 0 {
            return Ratio { num: 0, den: 1 };
        }
        Ratio {
            num: num / g,
            den: den / g,
        }
    }

    pub const fn int(n: u128) -> Ratio {
        Ratio { num: n, den: 1 }
    }

    pub const fn num(self) -> u128 {
        self.num
    }

    pub const fn den(self) -> u128 {
        self.den
    }

    pub const fn is_integer(self) -> bool {
        self.den == 1
    }

    pub const fn is_zero(self) -> bool {
        self.num == 0
    }

    pub fn checked_add(self, o: Ratio) -> Result<Ratio, Overflow> {
        let g = gcd(self.den, o.den);
        let l = (self.den / g).checked_mul(o.den).ok_or(Overflow)?;
        let a = self.num.checked_mul(l / self.den).ok_or(Overflow)?;
        let b = o.num.checked_mul(l / o.den).ok_or(Overflow)?;
        Ok(Ratio::new(a.checked_add(b).ok_or(Overflow)?, l))
    }

    /// `self - o`; `None` when the result would be negative.
    pub fn checked_sub(self, o: Ratio) -> Result<Option<Ratio>, Overflow> {
        if self < o {
            return Ok(None);
        }
        let g = gcd(self.den, o.den);
        let l = (self.den / g).checked_mul(o.den).ok_or(Overflow)?;
        let a = self.num.checked_mul(l / self.den).ok_or(Overflow)?;
        let b = o.num.checked_mul(l / o.den).ok_or(Overflow)?;
        Ok(Some(Ratio::new(a - b, l)))
    }

    pub fn checked_mul(self, o: Ratio) -> Result<Ratio, Overflow> {
        // cross-reduce first so the products stay small
        let g1 = gcd(self.num, o.den).max(1);
        let g2 = gcd(o.num, self.den).max(1);
        let num = (self.num / g1).checked_mul(o.num / g2).ok_or(Overflow)?;
        let den = (self.den / g2).checked_mul(o.den / g1).ok_or(Overflow)?;
        Ok(Ratio::new(num, den))
    }

    /// `self / o`; panics when `o` is zero (callers divide by dividers, which are never zero).
    pub fn checked_div(self, o: Ratio) -> Result<Ratio, Overflow> {
        assert!(!o.is_zero(), "Ratio divided by zero");
        self.checked_mul(Ratio {
            num: o.den,
            den: o.num,
        })
    }

    pub fn mul_int(self, n: u128) -> Result<Ratio, Overflow> {
        self.checked_mul(Ratio::int(n))
    }

    pub fn div_int(self, n: u128) -> Result<Ratio, Overflow> {
        self.checked_div(Ratio::int(n))
    }

    /// Largest integer ≤ self.
    pub const fn floor(self) -> u128 {
        self.num / self.den
    }

    /// Smallest integer ≥ self.
    pub const fn ceil(self) -> u128 {
        self.num.div_ceil(self.den)
    }

    /// For display only: never used in a check.
    pub fn approx_f64(self) -> f64 {
        self.num as f64 / self.den as f64
    }

    /// The exact decimal expansion when it terminates (den has only factors 2 and 5), else `None`.
    pub fn exact_decimal(self) -> Option<String> {
        let mut d = self.den;
        let (mut twos, mut fives) = (0u32, 0u32);
        while d.is_multiple_of(2) {
            d /= 2;
            twos += 1;
        }
        while d.is_multiple_of(5) {
            d /= 5;
            fives += 1;
        }
        if d != 1 {
            return None;
        }
        let digits = twos.max(fives);
        let int = self.num / self.den;
        let rem = self.num % self.den;
        if digits == 0 || rem == 0 {
            return Some(int.to_string());
        }
        // rem / den = frac / 10^digits, exact because den divides 10^digits
        let scale = 10u128.checked_pow(digits)?;
        let frac = rem.checked_mul(scale / self.den)?;
        let s = format!("{int}.{frac:0width$}", width = digits as usize);
        Some(s.trim_end_matches('0').to_string())
    }
}

impl Ord for Ratio {
    /// Exact comparison without overflow: integer parts first, then the reciprocals of the remainders (the
    /// continued-fraction walk).
    fn cmp(&self, other: &Self) -> Ordering {
        let (mut a, mut b, mut c, mut d) = (self.num, self.den, other.num, other.den);
        let mut flip = false;
        loop {
            let (qa, qc) = (a / b, c / d);
            if qa != qc {
                let o = qa.cmp(&qc);
                return if flip { o.reverse() } else { o };
            }
            let (ra, rc) = (a % b, c % d);
            match (ra == 0, rc == 0) {
                (true, true) => return Ordering::Equal,
                (true, false) => {
                    return if flip {
                        Ordering::Greater
                    } else {
                        Ordering::Less
                    };
                }
                (false, true) => {
                    return if flip {
                        Ordering::Less
                    } else {
                        Ordering::Greater
                    };
                }
                (false, false) => {
                    // ra/b vs rc/d  <=>  d/rc vs b/ra
                    (a, b, c, d) = (b, ra, d, rc);
                    flip = !flip;
                }
            }
        }
    }
}

impl PartialOrd for Ratio {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl fmt::Debug for Ratio {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}/{}", self.num, self.den)
    }
}

impl fmt::Display for Ratio {
    /// Exact decimal when it terminates, else `num/den`.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.exact_decimal() {
            Some(s) => f.write_str(&s),
            None => write!(f, "{}/{}", self.num, self.den),
        }
    }
}

/// A frequency in Hz, printed with a unit prefix and its exact digits: `519.999755859375 MHz`, or
/// `80/3 MHz (≈ 26.666667 MHz)` when the decimal doesn't terminate.
#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct Hz(pub Ratio);

impl Hz {
    pub const fn mhz(n: u128) -> Hz {
        Hz(Ratio::int(n * 1_000_000))
    }

    pub const fn khz(n: u128) -> Hz {
        Hz(Ratio::int(n * 1_000))
    }

    pub const fn from_int(n: u128) -> Hz {
        Hz(Ratio::int(n))
    }

    /// `n / 10` MHz, for limits like 42.5 or 137.5 MHz.
    pub const fn mhz_tenths(n: u128) -> Hz {
        Hz(Ratio::int(n * 100_000))
    }

    /// `n / 100` MHz, for limits like 1.17 MHz.
    pub const fn mhz_hundredths(n: u128) -> Hz {
        Hz(Ratio::int(n * 10_000))
    }

    pub const fn ratio(self) -> Ratio {
        self.0
    }
}

impl fmt::Display for Hz {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let r = self.0;
        let (div, unit) = if r >= Ratio::int(1_000_000) {
            (1_000_000, "MHz")
        } else if r >= Ratio::int(1_000) {
            (1_000, "kHz")
        } else {
            (1, "Hz")
        };
        let scaled = Ratio::new(r.num, r.den * div);
        match scaled.exact_decimal() {
            Some(s) => write!(f, "{s} {unit}"),
            None => write!(
                f,
                "{}/{} {unit} (≈ {:.6} {unit})",
                scaled.num,
                scaled.den,
                scaled.approx_f64()
            ),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reduces_and_adds() {
        let a = Ratio::new(6, 4);
        assert_eq!((a.num(), a.den()), (3, 2));
        let s = Ratio::int(173).checked_add(Ratio::new(2730, 8192)).unwrap();
        assert_eq!(s, Ratio::new(173 * 8192 + 2730, 8192));
        assert_eq!(
            Ratio::int(1).checked_sub(Ratio::new(1, 3)).unwrap(),
            Some(Ratio::new(2, 3))
        );
        assert_eq!(Ratio::new(1, 3).checked_sub(Ratio::int(1)).unwrap(), None);
    }

    #[test]
    fn compares_exactly() {
        assert!(Ratio::new(1, 3) < Ratio::new(1, 2));
        assert!(Ratio::new(2, 3) > Ratio::new(3, 5));
        assert_eq!(Ratio::new(2, 4).cmp(&Ratio::new(1, 2)), Ordering::Equal);
        // close values with large terms
        let a = Ratio::new(u128::MAX / 3, u128::MAX / 7);
        let b = Ratio::new(u128::MAX / 3 - 1, u128::MAX / 7);
        assert!(a > b);
        assert!(Ratio::int(5) > Ratio::new(9, 2));
        assert!(Ratio::new(9, 2) < Ratio::int(5));
    }

    #[test]
    fn b135_pll1_is_exact() {
        let ref_ck = Ratio::new(24_000_000, 8);
        let vco = ref_ck
            .checked_mul(Ratio::int(173).checked_add(Ratio::new(2730, 8192)).unwrap())
            .unwrap();
        assert_eq!(vco, Ratio::new(33_279_984_375, 64));
        assert_eq!(Hz(vco).to_string(), "519.999755859375 MHz");
        assert_eq!(
            Hz(Ratio::new(80_000_000, 3)).to_string(),
            "80/3 MHz (≈ 26.666667 MHz)"
        );
        assert_eq!(Hz::mhz_tenths(1375).to_string(), "137.5 MHz");
        assert_eq!(Hz::from_int(48_000_000).to_string(), "48 MHz");
    }
}
