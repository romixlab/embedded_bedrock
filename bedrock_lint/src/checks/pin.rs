//! What the PIN checks share: pin names and pin drivers

use crate::model::scan;
use syn::Expr;

/// A pin as the HALs name it: `PA5` (STM32), `PIN_12` (RP), `P0_13` (nRF)
pub fn is_pin_name(s: &str) -> bool {
    let b = s.as_bytes();
    let digits = |r: &[u8]| !r.is_empty() && r.len() <= 2 && r.iter().all(u8::is_ascii_digit);
    (b.len() >= 3 && b[0] == b'P' && (b'A'..=b'K').contains(&b[1]) && digits(&b[2..]))
        || s.strip_prefix("PIN_").is_some_and(|r| digits(r.as_bytes()))
        || (b.len() >= 4
            && b[0] == b'P'
            && (b[1] == b'0' || b[1] == b'1')
            && b[2] == b'_'
            && digits(&b[3..]))
}

/// GPIO driver types of embassy and the HAL crates
pub const DRIVERS: &[&str] = &["Output", "Input", "Flex", "OutputOpenDrain", "ExtiInput"];

/// `Output::new(..)`, `gpio::Flex::new(..)`: the driver type it builds
pub fn driver_new(e: &Expr) -> Option<String> {
    let name = scan::call_name(e)?;
    let mut segs = name.rsplit("::");
    let (last, ty) = (segs.next()?, segs.next()?);
    (last == "new" && DRIVERS.contains(&ty)).then(|| ty.to_string())
}
