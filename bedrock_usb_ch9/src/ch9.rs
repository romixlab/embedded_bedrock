//! USB 2.0 chapter 9 constants and the two standard requests `nusb` has no built-in wrapper for
//! (`GET_STATUS`, `SET_FEATURE`, `CLEAR_FEATURE`); `GET_DESCRIPTOR` and `SET_CONFIGURATION` use `nusb`'s own
//! `Device::get_descriptor` / `Device::set_configuration`, and alternate settings `Interface::set_alt_setting`.

use std::time::Duration;

use nusb::transfer::{ControlIn, ControlOut, ControlType, Recipient, TransferError};
use nusb::{Device, MaybeFuture};

/// `bRequest` field, USB 2.0 Table 9-4.
pub const GET_STATUS: u8 = 0;
pub const CLEAR_FEATURE: u8 = 1;
pub const SET_FEATURE: u8 = 3;

/// `wValue` of SET_FEATURE / CLEAR_FEATURE targeting an endpoint, USB 2.0 Table 9-6.
pub const ENDPOINT_HALT: u16 = 0;

/// `bDescriptorType`, USB 2.0 Table 9-5.
pub const DESC_BOS: u8 = 0x0f;

/// GET_STATUS (USB 2.0 §9.4.5): 2 bytes, bit 0 is the feature (Halt for an endpoint).
pub fn get_status(
    dev: &Device,
    recipient: Recipient,
    index: u16,
    timeout: Duration,
) -> Result<Vec<u8>, TransferError> {
    dev.control_in(
        ControlIn {
            control_type: ControlType::Standard,
            recipient,
            request: GET_STATUS,
            value: 0,
            index,
            length: 2,
        },
        timeout,
    )
    .wait()
}

/// SET_FEATURE (USB 2.0 §9.4.9).
pub fn set_feature(
    dev: &Device,
    recipient: Recipient,
    feature: u16,
    index: u16,
    timeout: Duration,
) -> Result<(), TransferError> {
    dev.control_out(
        ControlOut {
            control_type: ControlType::Standard,
            recipient,
            request: SET_FEATURE,
            value: feature,
            index,
            data: &[],
        },
        timeout,
    )
    .wait()
}

/// CLEAR_FEATURE (USB 2.0 §9.4.1).
pub fn clear_feature(
    dev: &Device,
    recipient: Recipient,
    feature: u16,
    index: u16,
    timeout: Duration,
) -> Result<(), TransferError> {
    dev.control_out(
        ControlOut {
            control_type: ControlType::Standard,
            recipient,
            request: CLEAR_FEATURE,
            value: feature,
            index,
            data: &[],
        },
        timeout,
    )
    .wait()
}

/// wLength values to probe a descriptor of `actual` bytes against: a short read, one byte either side of the
/// exact length, and the nearest multiples of the control endpoint's max packet size below and above `actual`
/// (R13: a descriptor whose length lands exactly on such a boundary needs a ZLP to end the transfer when more
/// was requested; one that doesn't land there ends on its own short packet either way).
pub fn wlength_candidates(actual: usize, ep0_max_packet: usize) -> Vec<u16> {
    let mut v = Vec::new();
    let mut push = |n: i64| {
        if let Ok(n) = u16::try_from(n.max(0))
            && !v.contains(&n)
        {
            v.push(n);
        }
    };
    push(1);
    push(actual as i64 - 1);
    push(actual as i64);
    push(actual as i64 + 1);
    if ep0_max_packet > 0 {
        let mp = ep0_max_packet as i64;
        let below = (actual as i64 / mp) * mp;
        push(below - mp);
        push(below);
        push(below + mp);
    }
    v.sort_unstable();
    v
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn candidates_bracket_the_actual_length() {
        let c = wlength_candidates(9, 64);
        assert_eq!(c, vec![0, 1, 8, 9, 10, 64]);
    }

    #[test]
    fn candidates_include_the_boundary_when_actual_is_a_multiple() {
        // the config descriptor can legitimately be exactly one max-packet multiple: that is the case R13 cares
        // about, so it must always be a candidate alongside the next multiple up.
        let c = wlength_candidates(64, 64);
        assert_eq!(c, vec![0, 1, 63, 64, 65, 128]);
    }

    #[test]
    fn no_duplicate_or_negative_candidates_for_a_tiny_descriptor() {
        let c = wlength_candidates(1, 64);
        // actual - 1 == 0 and the lower boundary is also 0: only one 0 in the list, and nothing negative crept in
        assert_eq!(c, vec![0, 1, 2, 64]);
    }
}
