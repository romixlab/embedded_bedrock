//! STM32H72x/73x (RM0468) and H74x/75x single-core (RM0433): addresses and how a reset is told from the ROM's jump.
//! Tested on the H725 (B135 rev B, 21 Oct 2026). Not for the H7A3/7B3/7B0 (RM0455: other RCC offsets, the ROM at
//! 0x1FF0_A000) nor the dual-core H745/H755 yet.
//!
//! ROM bootloaders before V9.3 on these parts lock the supply configuration in PWR_CR3 until power-off (AN2606
//! Table 134): what that means depends on the board's supply, so the firmware checks it after `early`
//! ([Entered::pwr_cr3] shows the state the ROM left).

/// RCC.
pub(crate) const RCC: u32 = 0x5802_4400;
/// RCC_CR.
pub(crate) const RCC_CR: u32 = RCC;
/// RCC_CFGR.
pub(crate) const RCC_CFGR: u32 = RCC + 0x10;
/// RCC_RSR: the reset flags.
pub(crate) const RCC_RSR: u32 = RCC + 0xD0;
/// RCC_APB4ENR: RTCAPBEN, bit 16.
pub(crate) const RCC_APB4ENR: u32 = RCC + 0xF4;
/// RCC_APB4ENR.RTCAPBEN.
pub(crate) const RTCAPBEN: u32 = 1 << 16;
/// The clock enable register and bits the backup registers need: the RTC's APB clock.
pub(crate) const BKP_CLOCK: (u32, u32) = (RCC_APB4ENR, RTCAPBEN);
/// PWR_CR1 (PWR at 0x5802_4800): DBP, bit 8.
pub(crate) const PWR_CR1: u32 = 0x5802_4800;
/// PWR_CSR1.
pub(crate) const PWR_CSR1: u32 = 0x5802_4804;
/// PWR_CR3: the supply configuration.
pub(crate) const PWR_CR3: u32 = 0x5802_480C;

/// RTC_BKP0R (RTC at 0x5800_4000, backup registers at offset 0x50..0xCC, RM0468 §51.7.20).
pub const RTC_BKP0R: u32 = 0x5800_4050;
/// Backup registers on the part.
pub const BKP_REGISTERS: usize = 32;
/// The backup register with the [BootWord](crate::BootWord): the last one, RTC_BKP31R. `bkp_cnt!` counters and the
/// board's own registers stay below it.
pub const BOOT_WORD: usize = BKP_REGISTERS - 1;

/// Address of backup register `index`.
pub const fn bkp_address(index: usize) -> u32 {
    RTC_BKP0R + 4 * index as u32
}

/// Where the ROM bootloader's vector table starts (AN2606 Rev 70, Table 133 for H72x/73x and Table 135 for
/// H74x/75x: "The bootloader start address is 0x1FF09800"): initial MSP, then the reset vector.
pub const ROM_BOOTLOADER: u32 = 0x1FF0_9800;

/// The 96-bit unique id.
pub const UID: u32 = 0x1FF1_E800;

/// RCC_CFGR.SWS after any reset: HSI (bits 5:3).
const SWS_HSI: u32 = 0;
/// RCC_CR.PLL1ON, PLL2ON, PLL3ON.
const RCC_CR_PLLS: u32 = 1 << 24 | 1 << 26 | 1 << 28;

/// What the chip looked like when the firmware's first instruction ran: the registers that tell a reset from the
/// ROM's jump, and the reset flags and supply state, for the log.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
#[repr(C)]
pub struct Entered {
    /// RCC_CR: oscillators and PLLs on.
    pub rcc_cr: u32,
    /// RCC_CFGR: SWS (bits 5:3) is the SYSCLK source, 0 the HSI.
    pub rcc_cfgr: u32,
    /// RCC_RSR: the reset flags (not yet cleared).
    pub rcc_rsr: u32,
    /// PWR_CR3: the supply configuration.
    pub pwr_cr3: u32,
    /// PWR_CSR1: ACTVOSRDY (bit 13) and the voltage scaling.
    pub pwr_csr1: u32,
    /// SCB.VTOR.
    pub vtor: u32,
    /// The NVIC's ISER0..7 ORed: non-zero when an interrupt is enabled.
    pub nvic_enabled: u32,
}

impl Entered {
    /// The chip is as a reset leaves it: SYSCLK from the HSI and VTOR on the running image's vector table at
    /// `own_vectors`, no PLL on, no interrupt enabled. The ROM's leave (DfuSe leave, the Go command) jumps with the
    /// system clock from a PLL, SysTick and the USB interrupt on and VTOR on the ROM's table (AN2606 §4: "the
    /// peripheral registers used by the bootloader are not initialized to their default reset values before jumping
    /// to the user application").
    pub const fn from_reset(&self, own_vectors: u32) -> bool {
        (self.rcc_cfgr >> 3) & 0b111 == SWS_HSI
            && self.vtor == own_vectors
            && self.rcc_cr & RCC_CR_PLLS == 0
            && self.nvic_enabled == 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const OWN: u32 = 0x0800_0000;

    #[test]
    fn addresses() {
        assert_eq!(bkp_address(0), 0x5800_4050);
        assert_eq!(bkp_address(BOOT_WORD), 0x5800_40CC);
    }

    #[test]
    fn reset_or_jump() {
        // B135 rev B after a reset
        let reset = Entered {
            rcc_cr: 0x0000_0025,
            rcc_cfgr: 0,
            rcc_rsr: 0x0042_0000,
            pwr_cr3: 0x0000_0006,
            pwr_csr1: 0,
            vtor: 0x0800_0000,
            nvic_enabled: 0,
        };
        assert!(reset.from_reset(OWN));
        // the ROM leaving DFU: PLL1 on and the system clock from it, OTG_FS enabled, its vector table
        assert!(
            !Entered {
                rcc_cr: reset.rcc_cr | 1 << 24,
                ..reset
            }
            .from_reset(OWN)
        );
        assert!(
            !Entered {
                rcc_cfgr: 3 << 3,
                ..reset
            }
            .from_reset(OWN)
        );
        assert!(
            !Entered {
                nvic_enabled: 1 << 13,
                ..reset
            }
            .from_reset(OWN)
        );
        assert!(
            !Entered {
                vtor: ROM_BOOTLOADER,
                ..reset
            }
            .from_reset(OWN)
        );
        // an image behind a bootloader: HSI, VTOR on the image
        assert!(
            Entered {
                vtor: 0x0806_0000,
                ..reset
            }
            .from_reset(0x0806_0000)
        );
    }
}
