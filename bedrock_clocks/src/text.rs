//! Text the emitters of every family share: register words in hex, exact frequencies as `Fixed` consts, errors.

use std::fmt::Write as _;

use exact_units::prefix::One;
use exact_units::units::Hertz;
use exact_units::{Exact, Rounding};

pub fn hex(v: u32) -> String {
    format!("0x{:04X}_{:04X}", v >> 16, v & 0xFFFF)
}

/// `Fixed<Hertz, Rational<1, D>, u64>` const for an exact frequency: D is its denominator in lowest terms, so the
/// `Fixed` holds it exactly.
pub fn fixed(name: &str, h: Exact<Hertz>, doc: &str, out: &mut String) {
    let r = h.get();
    let floor = h
        .to_rounded::<One, u32>(Rounding::Floor)
        .expect("a clock fits u32 Hz");
    let _ = writeln!(out, "/// {doc}: {h}");
    let _ = writeln!(
        out,
        "pub const {name}: Fixed<Hertz, Rational<1, {}>, u64> = Fixed::new({});",
        r.denom().expect("a clock's denominator fits u128"),
        r.numer().expect("a clock fits u128 Hz")
    );
    let _ = writeln!(
        out,
        "/// {doc}, rounded down to whole Hz (lossy, for drivers that take integer Hz)"
    );
    let _ = writeln!(out, "pub const {name}_HZ_FLOOR: u32 = {};", floor.get());
}

/// Signed error of `f` against `target`, exactly: `-0.244140625 ppm`, or `exact`.
pub fn error_text(f: Exact<Hertz>, target: Exact<Hertz>) -> String {
    match f.rel_error(target) {
        Some(e) if e.is_zero() => "exact".into(),
        Some(e) => e.to_string(),
        None => "error too large to show".into(),
    }
}
