#![cfg_attr(not(test), no_std)]
//! Firmware updates through the STM32 ROM bootloader (DEV-16): the early boot that runs first thing in `main`, before
//! clocks and embassy, and the reboots `device.boot.reboot` asks for. Extracted from Donguru (typec_dongle FW-12,
//! FW-45, STM32G0B1) and B135 (BOOT-4, BOOT-5, BOOT-9, BOOT-10, STM32H725).
//!
//! The firmware sits alone at the start of flash; the ROM's USB DFU (`ww dfu stm32 flash`) is the updater.
//!
//! - `reboot(Updater)` writes a request into the [boot word](BootWord), kept in the family's last backup register
//!   ([g0::BOOT_WORD], [h7::BOOT_WORD]), and resets. The next boot's `early` finds it, clears it and jumps into the
//!   ROM bootloader from the reset state (AN2606 §4.1: peripheral clocks, PLLs and interrupts as after reset).
//! - The ROM leaves DFU with a jump to the firmware, its clocks, USB and vector table still set up: `early` tells
//!   that start from a reset (`Entered::from_reset` of [g0], [h7]) and resets once, so the HAL starts from the reset
//!   state. A second such start in a row goes on (a reset loop is worse), and says so.
//! - A boot loop: every boot counts in the boot word ([BootWord::count_boot]), a healthy one clears the count
//!   (`mark_healthy`, or the `embassy` feature's `healthy_watch`). After [BOOT_LOOP_LIMIT] boots without that mark,
//!   `early` returns [Start::Rescue]: the firmware brings up what the ROM's USB needs (a hub, a supply) or nothing,
//!   and calls `rescue`, which resets into the ROM bootloader. The next firmware reports it as
//!   `Outcome::Recovery` (`fell_back`, [dfu::node]).
//! - The USB serial number: the one the ROM reports in DFU mode ([rom_serial]), so a host follows the board into
//!   the ROM and back.
//!
//! The decisions are plain functions on register values, host-tested; the register accesses are in `rt` behind a
//! family feature (`stm32g0`, `stm32h7`), with the RM addresses and no HAL, so the code runs before the HAL's init
//! and in a bootloader alike.
//!
//! The backup registers survive a system reset, not a power cycle without VBAT, which clears the word: no request,
//! no count, which is right (a power cycle is no crash). ES0548 2.2.11 (G0B1, not fixed): a supply that drops to
//! 100-700 mV and rises again can leave the backup domain unreset with random contents. The word's 16-bit tag makes
//! such contents read as an empty word but for 1 in 65536; then at worst one ROM session or one rescue follows, and
//! a reset (or the host's `ww dfu stm32 leave`) boots the firmware again.

#[cfg(feature = "ww-dfu")]
pub mod dfu;
// the register addresses only the runtime of that family uses
#[cfg_attr(not(feature = "stm32g0"), allow(dead_code))]
pub mod g0;
#[cfg_attr(not(feature = "stm32h7"), allow(dead_code))]
pub mod h7;
#[cfg(any(feature = "stm32g0", feature = "stm32h7"))]
mod rt;
#[cfg(test)]
mod tests;

#[cfg(any(feature = "stm32g0", feature = "stm32h7"))]
pub use rt::*;

#[cfg(all(feature = "stm32g0", feature = "stm32h7"))]
compile_error!("bedrock_rom_boot: enable one chip family feature, `stm32g0` or `stm32h7`");

/// Upper half of a valid boot word: anything else (0 after a backup domain reset, noise) is [BootWord::default].
const TAG: u32 = 0xB007_0000;
/// Bit of [BootWord::rom_request].
const ROM_REQUEST: u32 = 1 << 15;
/// Bit of [BootWord::fell_back].
const FELL_BACK: u32 = 1 << 14;
/// Bits of [BootWord::unhealthy_boots].
const UNHEALTHY_BOOTS: u32 = 0xF;

/// Boots in a row without a healthy mark after which the next boot is the rescue: three failed starts, the fourth
/// goes to the ROM bootloader.
pub const BOOT_LOOP_LIMIT: u8 = 3;

/// What the firmware keeps for its next boot in the family's boot register. Survives a system reset, not a power
/// cycle without VBAT.
///
/// | bits  | field                                    | set by                                     | cleared by                         |
/// |-------|------------------------------------------|--------------------------------------------|------------------------------------|
/// | 31:16 | `0xB007`, the tag                        | every write                                | a backup domain reset              |
/// | 15    | [rom_request](Self::rom_request)         | `reboot(Updater)`, the rescue boot         | the early boot, before the jump    |
/// | 14    | [fell_back](Self::fell_back)             | the rescue boot                            | `reboot(Updater)`, a power cycle   |
/// | 13:4  | free                                     |                                            |                                    |
/// | 3:0   | [unhealthy_boots](Self::unhealthy_boots) | each boot's [count_boot](Self::count_boot) | the [healthy](Self::healthy) mark  |
///
/// The same word as Donguru's (typec_dongle FW-12, FW-45): a firmware switching to this crate keeps reading what the
/// one before it wrote.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub struct BootWord {
    /// Go to the ROM bootloader at the next boot.
    pub rom_request: bool,
    /// The ROM bootloader took over after a boot loop: the last update is reported as `Outcome::Recovery` until the
    /// next requested update or a power cycle.
    pub fell_back: bool,
    /// Boots since the last healthy mark, this one included once counted (up to 15).
    pub unhealthy_boots: u8,
}

/// How the firmware starts, [BootWord::count_boot]'s decision.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum Start {
    /// The firmware as usual.
    App,
    /// [BOOT_LOOP_LIMIT] boots in a row without a healthy mark: bring up what the ROM's USB needs, if anything,
    /// then reset into the ROM bootloader (`rescue`).
    Rescue,
}

/// Where a reboot goes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum Reboot {
    /// The firmware again.
    App,
    /// The ROM bootloader: STM32 USB DFU; on RP2040 / RP2350 the firmware's own `boot.rs` enters BOOTSEL (PICOBOOT).
    Rom,
}

impl BootWord {
    /// The word as read from the register: without the tag it holds nothing.
    pub const fn from_word(word: u32) -> Self {
        if word & 0xFFFF_0000 != TAG {
            return Self {
                rom_request: false,
                fell_back: false,
                unhealthy_boots: 0,
            };
        }
        Self {
            rom_request: word & ROM_REQUEST != 0,
            fell_back: word & FELL_BACK != 0,
            unhealthy_boots: (word & UNHEALTHY_BOOTS) as u8,
        }
    }

    /// The word to write to the register.
    pub const fn word(self) -> u32 {
        let count = if self.unhealthy_boots as u32 > UNHEALTHY_BOOTS {
            UNHEALTHY_BOOTS
        } else {
            self.unhealthy_boots as u32
        };
        TAG | if self.rom_request { ROM_REQUEST } else { 0 }
            | if self.fell_back { FELL_BACK } else { 0 }
            | count
    }

    /// What the early boot does with the word it read: the word to write back and how to go on. A request is
    /// taken (one request, one ROM session: the ROM's leave comes back without it) and the jump follows; else this
    /// boot counts.
    pub const fn at_boot(self) -> (Self, Option<Start>) {
        if self.rom_request {
            (self.take_request(), None)
        } else {
            let (next, start) = self.count_boot();
            (next, Some(start))
        }
    }

    /// Counts a boot without a ROM request: the word to write back and how to start. After [BOOT_LOOP_LIMIT] boots
    /// without a [healthy](Self::healthy) mark this one is the rescue, and the count starts over: a rescue that
    /// itself crashes gives the firmware that many tries again before the next one.
    pub const fn count_boot(self) -> (Self, Start) {
        if self.unhealthy_boots >= BOOT_LOOP_LIMIT {
            (
                Self {
                    unhealthy_boots: 0,
                    ..self
                },
                Start::Rescue,
            )
        } else {
            (
                Self {
                    unhealthy_boots: self.unhealthy_boots + 1,
                    ..self
                },
                Start::App,
            )
        }
    }

    /// This boot is healthy: the count starts over.
    pub const fn healthy(self) -> Self {
        Self {
            unhealthy_boots: 0,
            ..self
        }
    }

    /// What the rescue boot writes before its reset: into the ROM bootloader, remembering why.
    pub const fn rescue() -> Self {
        Self {
            rom_request: true,
            fell_back: true,
            unhealthy_boots: 0,
        }
    }

    /// What a reboot writes. A requested update ends the story of an earlier fallback; a reboot into the firmware
    /// keeps it and the count.
    pub const fn reboot(self, into: Reboot) -> Self {
        let rom = matches!(into, Reboot::Rom);
        Self {
            rom_request: rom,
            fell_back: self.fell_back && !rom,
            ..self
        }
    }

    /// What the early boot writes before it jumps to the requested ROM bootloader: the request goes, the rest
    /// stays.
    pub const fn take_request(self) -> Self {
        Self {
            rom_request: false,
            ..self
        }
    }
}

/// Length of [rom_serial] in ASCII.
pub const SERIAL_LEN: usize = 12;

/// The USB serial number the STM32 ROM bootloader reports in DFU mode for the 96-bit unique id `uid` as the chip
/// stores it (12 bytes, little-endian words UID[31:0], UID[63:32], UID[95:64]; embassy's `uid::uid()`), in the form
/// of ST's USB device library (wire_weaver `ww_stm32_dfu::rom_serial`): the sum of the first and third word as 8 hex
/// digits, then the upper 16 bits of the second word as 4, upper case. The same on G0 and H7.
pub fn rom_serial(uid: &[u8; 12]) -> [u8; SERIAL_LEN] {
    let word =
        |i: usize| u32::from_le_bytes([uid[4 * i], uid[4 * i + 1], uid[4 * i + 2], uid[4 * i + 3]]);
    let value = ((word(0).wrapping_add(word(2)) as u64) << 16) | (word(1) >> 16) as u64;
    let mut out = [0; SERIAL_LEN];
    for (i, digit) in out.iter_mut().enumerate() {
        let nibble = (value >> (4 * (SERIAL_LEN - 1 - i))) & 0xF;
        *digit = b"0123456789ABCDEF"[nibble as usize];
    }
    out
}
