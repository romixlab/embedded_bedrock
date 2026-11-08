//! Values to write to `USB_CHEPnR`, computed from the value just read: no read-modify-write (R1), toggles as
//! current XOR target (R2). Pure functions on `u32`, so the rules are tested apart from any register.

use super::regs::chep::*;

/// `STATRX` / `STATTX` (RM0444 Tables 233 and 234).
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum Stat {
    Disabled = 0b00,
    Stall = 0b01,
    Nak = 0b10,
    Valid = 0b11,
}

impl Stat {
    pub const fn from_bits(bits: u32) -> Stat {
        match bits & 0b11 {
            0b00 => Stat::Disabled,
            0b01 => Stat::Stall,
            0b10 => Stat::Nak,
            _ => Stat::Valid,
        }
    }
}

pub const fn stat_tx(r: u32) -> Stat {
    Stat::from_bits(r >> STATTX_SHIFT)
}

pub const fn stat_rx(r: u32) -> Stat {
    Stat::from_bits(r >> STATRX_SHIFT)
}

/// The write that changes nothing: 1 in every `rc_w0` bit, 0 in every toggle bit, the `rw` bits as read (R1).
/// RM0444 §37.6.7: "all bits affected by this problem have an 'invariant' value that must be used whenever their
/// modification is not required".
pub const fn invariant(r: u32) -> u32 {
    (r & !(TOGGLE | RC_W0)) | RC_W0
}

/// `w` with the toggles that take STATTX from what `r` holds to `target` (R2).
pub const fn to_stat_tx(w: u32, r: u32, target: Stat) -> u32 {
    w | (((stat_tx(r) as u32) ^ (target as u32)) << STATTX_SHIFT)
}

/// `w` with the toggles that take STATRX from what `r` holds to `target` (R2).
pub const fn to_stat_rx(w: u32, r: u32, target: Stat) -> u32 {
    w | (((stat_rx(r) as u32) ^ (target as u32)) << STATRX_SHIFT)
}

/// `w` with the toggle that takes DTOGTX from what `r` holds to `value`.
pub const fn to_dtog_tx(w: u32, r: u32, value: bool) -> u32 {
    if (r & DTOGTX != 0) != value {
        w | DTOGTX
    } else {
        w
    }
}

/// `w` with the toggle that takes DTOGRX from what `r` holds to `value`.
pub const fn to_dtog_rx(w: u32, r: u32, value: bool) -> u32 {
    if (r & DTOGRX != 0) != value {
        w | DTOGRX
    } else {
        w
    }
}

/// Clears VTTX and nothing else.
pub const fn clear_vttx(r: u32) -> u32 {
    invariant(r) & !VTTX
}

/// Clears VTRX and nothing else.
pub const fn clear_vtrx(r: u32) -> u32 {
    invariant(r) & !VTRX
}

/// The `rw` part of a register as the driver wants it: address `ea`, type `utype`, `EPKIND`.
pub const fn config(ea: u8, utype: u32, kind: bool) -> u32 {
    (ea as u32 & EA) | (utype << UTYPE_SHIFT) | if kind { EPKIND } else { 0 }
}

/// The write that sets the `rw` part to `config` and keeps every hardware flag and toggle bit: [invariant] with
/// other `rw` bits.
pub const fn with_config(r: u32, config: u32) -> u32 {
    (invariant(r) & !(EA | UTYPE | EPKIND)) | config
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn invariant_keeps_flags_and_toggles_nothing() {
        // VTRX and VTTX set by the hardware, STAT and DTOG anything: the invariant write has 1 in both flags and 0
        // in every toggle bit
        for r in [
            0u32,
            0xffff_ffff,
            VTRX | VTTX | 0x3030,
            DTOGRX | DTOGTX | 5 | (1 << 9),
        ] {
            let w = invariant(r);
            assert_eq!(w & RC_W0, RC_W0, "{r:#x}");
            assert_eq!(w & TOGGLE, 0, "{r:#x}");
            assert_eq!(w & (EA | UTYPE | EPKIND), r & (EA | UTYPE | EPKIND));
        }
    }

    #[test]
    fn stat_toggles() {
        for from in [Stat::Disabled, Stat::Stall, Stat::Nak, Stat::Valid] {
            for to in [Stat::Disabled, Stat::Stall, Stat::Nak, Stat::Valid] {
                let r = (from as u32) << STATTX_SHIFT | (from as u32) << STATRX_SHIFT;
                // toggle semantics: new = old ^ written
                assert_eq!(stat_tx(r ^ to_stat_tx(0, r, to)), to);
                assert_eq!(stat_rx(r ^ to_stat_rx(0, r, to)), to);
                // and the other direction is left alone
                assert_eq!(to_stat_tx(0, r, to) & STATRX, 0);
            }
        }
    }

    #[test]
    fn clear_one_flag() {
        assert_eq!(clear_vttx(VTRX | VTTX) & RC_W0, RC_W0 & !VTTX);
        assert_eq!(clear_vtrx(VTRX | VTTX) & RC_W0, RC_W0 & !VTRX);
    }
}
