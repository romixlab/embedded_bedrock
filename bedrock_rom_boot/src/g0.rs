//! STM32G0 (RM0444): addresses and how a reset is told from the ROM's jump. Tested on the G0B1 (Donguru B153A,
//! 21 Oct 2026); the other G0 parts have the same registers and the ROM at the same address.

/// RCC.
pub(crate) const RCC: u32 = 0x4002_1000;
/// RCC_CFGR.
pub(crate) const RCC_CFGR: u32 = RCC + 0x08;
/// RCC_APBENR1: PWREN (bit 28), RTCAPBEN (bit 10), USBEN (bit 13).
pub(crate) const RCC_APBENR1: u32 = RCC + 0x3C;
/// RCC_APBENR1.PWREN.
pub(crate) const PWREN: u32 = 1 << 28;
/// RCC_APBENR1.RTCAPBEN: the RTC and TAMP APB clock.
pub(crate) const RTCAPBEN: u32 = 1 << 10;
/// RCC_APBENR1.USBEN.
const USBEN: u32 = 1 << 13;
/// PWR_CR1 (PWR at 0x4000_7000): DBP, bit 8.
pub(crate) const PWR_CR1: u32 = 0x4000_7000;
/// The clock enable register and bits the backup registers need: PWR and RTC / TAMP.
pub(crate) const BKP_CLOCK: (u32, u32) = (RCC_APBENR1, PWREN | RTCAPBEN);

/// TAMP_BKP0R (TAMP at 0x4000_B000, backup registers from offset 0x100, RM0444 §31.6.12).
pub const TAMP_BKP0R: u32 = 0x4000_B100;
/// Backup registers on the part.
pub const BKP_REGISTERS: usize = 5;
/// The backup register with the [BootWord](crate::BootWord): the last one, TAMP_BKP4R. `bkp_cnt!` counters get the
/// four below it (`BKPSRAM` in `memory.x` ends below it).
pub const BOOT_WORD: usize = BKP_REGISTERS - 1;

/// Address of backup register `index`.
pub const fn bkp_address(index: usize) -> u32 {
    TAMP_BKP0R + 4 * index as u32
}

/// Where the ROM bootloader's vector table is (AN2606 Rev 70, G0 sections: system memory "starting from address
/// 0x1FFF0000, contain the bootloader firmware"): initial MSP, then the reset vector.
pub const ROM_BOOTLOADER: u32 = 0x1FFF_0000;

/// The 96-bit unique id.
pub const UID: u32 = 0x1FFF_7590;

/// RCC_CFGR.SWS when the system clock is HSISYS, as after any reset (RM0444 §5.4.3, bits 5:3).
const SWS_HSISYS: u32 = 0;

/// The registers that say how the firmware was entered, read before anything is changed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
#[repr(C)]
pub struct Entered {
    /// RCC_CFGR.
    pub rcc_cfgr: u32,
    /// RCC_APBENR1 (USBEN, bit 13, among the peripheral clocks).
    pub rcc_apbenr1: u32,
    /// SCB.VTOR.
    pub vtor: u32,
    /// NVIC_ISER: interrupts enabled.
    pub nvic_iser: u32,
}

impl Entered {
    /// The firmware started from a reset: system clock HSISYS, the USB clock off, no interrupt enabled and VTOR on
    /// address 0 (its reset value, flash aliased there) or on the firmware's own vector table at `own_vectors`. A
    /// jump from the ROM bootloader's leave fails at least one: it runs from HSI48 or the PLL, with its USB and its
    /// vector table.
    pub const fn from_reset(&self, own_vectors: u32) -> bool {
        let sws = (self.rcc_cfgr >> 3) & 0b111;
        sws == SWS_HSISYS
            && self.rcc_apbenr1 & USBEN == 0
            && self.nvic_iser == 0
            && (self.vtor == 0 || self.vtor == own_vectors)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const OWN: u32 = 0x0800_0000;

    fn reset() -> Entered {
        Entered {
            rcc_cfgr: 0,
            rcc_apbenr1: 0,
            vtor: 0,
            nvic_iser: 0,
        }
    }

    #[test]
    fn addresses() {
        // Donguru's TAMP_BKP4R
        assert_eq!(bkp_address(BOOT_WORD), 0x4000_B110);
    }

    #[test]
    fn from_reset() {
        assert!(reset().from_reset(OWN));
        assert!(
            Entered {
                vtor: OWN,
                ..reset()
            }
            .from_reset(OWN)
        );
    }

    #[test]
    fn jumped_from_the_rom() {
        // each fact alone gives the jump away
        assert!(
            !Entered {
                vtor: ROM_BOOTLOADER,
                ..reset()
            }
            .from_reset(OWN)
        );
        assert!(
            !Entered {
                rcc_cfgr: 0b010 << 3,
                ..reset()
            }
            .from_reset(OWN)
        );
        assert!(
            !Entered {
                rcc_apbenr1: USBEN,
                ..reset()
            }
            .from_reset(OWN)
        );
        assert!(
            !Entered {
                nvic_iser: 1 << 8,
                ..reset()
            }
            .from_reset(OWN)
        );
        // SW (bits 2:0) asking for another clock does not count, SWS (5:3) does
        assert!(
            Entered {
                rcc_cfgr: 0b010,
                ..reset()
            }
            .from_reset(OWN)
        );
    }
}
