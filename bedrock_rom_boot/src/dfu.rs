//! `device.boot` (`ww_dfu::BootControl`) and `device.dfu.node` (`ww_dfu::UpdateNode`) for a firmware the ROM
//! bootloader updates in place (feature `ww-dfu`): the boot state is always `Confirmed`, there is nothing to revert
//! to, `reboot(Updater)` enters the ROM bootloader. The server's handlers forward here:
//!
//! ```ignore
//! fn device_boot_boot_state(&mut self, _cx: ..) -> RpcResult<BootState> {
//!     Ready(bedrock_rom_boot::dfu::boot_state())
//! }
//! fn device_boot_confirm(&mut self, _cx: ..) -> RpcResult<Result<(), Error>> {
//!     Ready(bedrock_rom_boot::dfu::confirm())
//! }
//! fn device_boot_revert(&mut self, _cx: ..) -> RpcResult<Result<(), Error>> {
//!     Ready(bedrock_rom_boot::dfu::revert())
//! }
//! fn device_boot_reboot(&mut self, _cx: .., into: BootTarget) -> RpcResult<Result<(), Error>> {
//!     // resets REPLY_MS later, from the task running reboot_when_asked (features `embassy` and a family)
//!     bedrock_rom_boot::request_reboot(bedrock_rom_boot::dfu::reboot_target(into));
//!     Ready(Ok(()))
//! }
//! fn device_dfu_node_node(&mut self, _cx: ..) -> RpcResult<NodeInfo<'_>> {
//!     Ready(bedrock_rom_boot::dfu::node("Nucleo G0B1RE", bedrock_rom_boot::fell_back()))
//! }
//! ```

use crate::Reboot;
use wire_weaver::prelude::MaxLen;
use ww_dfu::{
    BootState, BootTarget, Error, LastResult, NodeInfo, Offers, Outcome, Profile, Role, Seal,
};

/// `BootControl::boot_state`: the running image is the only one, always confirmed.
pub const fn boot_state() -> BootState {
    BootState::Confirmed
}

/// `BootControl::confirm`: nothing on trial, `Ok` without effect.
pub const fn confirm() -> Result<(), Error> {
    Ok(())
}

/// `BootControl::revert`: no previous image to go back to (`ww_dfu`: `Unsupported` on [Profile::InPlace]).
pub const fn revert() -> Result<(), Error> {
    Err(Error::Unsupported)
}

/// Where `BootControl::reboot` goes: the updater is the ROM bootloader.
pub const fn reboot_target(into: BootTarget) -> Reboot {
    match into {
        BootTarget::App => Reboot::App,
        BootTarget::Updater => Reboot::Rom,
    }
}

/// `UpdateNode::node`: the firmware in place, updated by the ROM bootloader (`device.boot` only, like an RP2350 with
/// BOOTSEL), images not sealed (no product id, version, floor or manifest). `product` is cut to
/// `ww_dfu::MAX_PRODUCT_LEN` bytes at a character boundary; `fell_back` (the runtime's `fell_back`) makes the last
/// result `Recovery`: the ROM took over after a boot loop. Requested updates are not tracked.
pub fn node(product: &str, fell_back: bool) -> NodeInfo<'_> {
    let mut len = product.len().min(ww_dfu::MAX_PRODUCT_LEN);
    while !product.is_char_boundary(len) {
        len -= 1;
    }
    NodeInfo {
        role: Role::App,
        profile: Profile::InPlace,
        product: MaxLen::new_unchecked(&product[..len]),
        geometry_id: 0,
        product_id: 0,
        seal: Seal::Unsigned,
        app_version: None,
        version_floor: 0,
        manifest_digest: None,
        boot_state: BootState::Confirmed,
        last_result: LastResult {
            outcome: if fell_back {
                Outcome::Recovery
            } else {
                Outcome::Nothing
            },
            version: 0,
        },
        offers: Offers {
            boot_control: true,
            inventory: false,
            stage: false,
            stage_bulk: false,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn in_place() {
        assert_eq!(boot_state(), BootState::Confirmed);
        assert!(confirm().is_ok());
        assert!(matches!(revert(), Err(Error::Unsupported)));
        assert_eq!(reboot_target(BootTarget::Updater), Reboot::Rom);
        assert_eq!(reboot_target(BootTarget::App), Reboot::App);
    }

    #[test]
    fn node_info() {
        let n = node("Nucleo G0B1RE", false);
        assert_eq!(n.profile, Profile::InPlace);
        assert_eq!(n.role, Role::App);
        assert_eq!(n.boot_state, BootState::Confirmed);
        assert_eq!(*n.product, "Nucleo G0B1RE");
        assert_eq!(n.last_result.outcome, Outcome::Nothing);
        assert!(n.offers.boot_control && !n.offers.stage && !n.offers.inventory);
        assert_eq!(node("x", true).last_result.outcome, Outcome::Recovery);
        // too long: cut at a character boundary within the limit
        let long = "ä".repeat(20);
        let n = node(&long, false);
        assert_eq!(n.product.len(), ww_dfu::MAX_PRODUCT_LEN);
        let long = format!("a{}", "ä".repeat(20));
        assert_eq!(
            node(&long, false).product.len(),
            ww_dfu::MAX_PRODUCT_LEN - 1
        );
    }
}
