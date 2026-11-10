//! The RM0444 G0B1 / G0C1 family.
//!
//! [apply] follows RM0444 Rev 5:
//! 0. Not fresh from reset: back to HSISYS and the PLL off, as the PLL is configured only while off (§5.2.4).
//! 1. PWR clock on (RCC_APBENR1 PWREN); Range 2 to Range 1 before the clocks go up: VOS, then VOSF clear (§4.1.4).
//! 2. Oscillators: HSEBYP while HSE is off (§5.4.1), then the ON bits and their ready flags.
//! 3. The PLL: RCC_PLLCFGR, PLLON, PLLRDY (§5.2.4).
//! 4. More flash wait states before a faster clock, read back (§3.3.4 "Increasing the CPU frequency").
//! 5. RCC_CFGR SW, HPRE, PPRE, then SWS (§5.4.3).
//! 6. The plan's FLASH_ACR (fewer wait states after a slower clock, PRFTEN, ICEN), read back (§3.3.4).
//! 7. Range 1 to Range 2 once SYSCLK is at most 16 MHz and the wait states are set (§4.1.4).
//! 8. Kernel muxes RCC_CCIPR / CCIPR2.
//! 9. The CRS: its clock, CRS_CFGR while CEN is off, then CRS_CR (§6.7.1-6.7.2).
//! 10. Oscillators the plan doesn't use off (HSI16).

/// The register words of a plan, as `bedrock_clocks` emits them in `clocks.rs` (`PLAN`).
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub struct Plan {
    /// PWR_CR1 VOS field in place (bits 10:9).
    pub pwr_cr1_vos: u32,
    /// FLASH_ACR LATENCY, PRFTEN and ICEN.
    pub flash_acr: u32,
    /// RCC_CR oscillator bits to set (HSION and HSIDIV, HSEON, HSEBYP, HSI48ON).
    pub rcc_cr_osc: u32,
    /// RCC_CR PLLON, or 0.
    pub rcc_cr_pll_on: u32,
    /// RCC_CR oscillator ON bits to clear at the end.
    pub rcc_cr_off: u32,
    pub pllcfgr: u32,
    /// SW, HPRE and PPRE.
    pub cfgr: u32,
    pub ccipr: u32,
    pub ccipr2: u32,
    /// 0 without the CRS.
    pub crs_cfgr: u32,
    /// AUTOTRIMEN and CEN with TRIM at its reset value; 0 without the CRS.
    pub crs_cr: u32,
}

/// RCC, PWR, FLASH and CRS register addresses (RM0444 §2.2.2 memory map, p.62-63; held to stm32-metapac in a test).
pub mod addr {
    pub const RCC: u32 = 0x4002_1000;
    pub const PWR: u32 = 0x4000_7000;
    pub const FLASH: u32 = 0x4002_2000;
    pub const CRS: u32 = 0x4000_6C00;

    pub const RCC_CR: u32 = RCC;
    pub const RCC_CFGR: u32 = RCC + 0x08;
    pub const RCC_PLLCFGR: u32 = RCC + 0x0C;
    pub const RCC_APBENR1: u32 = RCC + 0x3C;
    pub const RCC_CCIPR: u32 = RCC + 0x54;
    pub const RCC_CCIPR2: u32 = RCC + 0x58;

    pub const PWR_CR1: u32 = PWR;
    pub const PWR_SR2: u32 = PWR + 0x14;

    pub const FLASH_ACR: u32 = FLASH;

    pub const CRS_CR: u32 = CRS;
    pub const CRS_CFGR: u32 = CRS + 0x04;
}

/// Bits of RCC_CR (§5.4.1), RCC_CFGR (§5.4.3), RCC_APBENR1 (§5.4.15), PWR_SR2 (§4.4.6), FLASH_ACR (§3.7.1) and CRS_CR
/// (§6.7.1).
pub mod bits {
    pub const CR_HSION: u32 = 1 << 8;
    pub const CR_HSIRDY: u32 = 1 << 10;
    pub const CR_HSIDIV: u32 = 0b111 << 11;
    pub const CR_HSEON: u32 = 1 << 16;
    pub const CR_HSERDY: u32 = 1 << 17;
    pub const CR_HSEBYP: u32 = 1 << 18;
    pub const CR_HSI48ON: u32 = 1 << 22;
    pub const CR_HSI48RDY: u32 = 1 << 23;
    pub const CR_PLLON: u32 = 1 << 24;
    pub const CR_PLLRDY: u32 = 1 << 25;

    pub const CFGR_SW: u32 = 0b111;
    pub const CFGR_SWS: u32 = 0b111 << 3;
    pub const CFGR_HPRE: u32 = 0b1111 << 8;
    pub const CFGR_PPRE: u32 = 0b111 << 12;

    pub const APBENR1_CRSEN: u32 = 1 << 16;
    pub const APBENR1_PWREN: u32 = 1 << 28;

    pub const PWR_CR1_VOS: u32 = 0b11 << 9;
    pub const PWR_SR2_VOSF: u32 = 1 << 10;

    pub const ACR_LATENCY: u32 = 0b111;
    /// LATENCY, PRFTEN and ICEN: the fields a plan owns (EMPTY and DBG_SWEN aren't the clocks').
    pub const ACR_PLAN: u32 = ACR_LATENCY | (1 << 8) | (1 << 9);

    pub const CRS_CR_CEN: u32 = 1 << 5;
    pub const CRS_CR_AUTOTRIMEN: u32 = 1 << 6;

    /// RCC_PLLCFGR without its reserved bits.
    pub const PLLCFGR_ALL: u32 = 0xFF3F_7F73;
    /// The RCC_CCIPR fields a plan sets (all but CECSEL).
    pub const CCIPR_PLAN: u32 = 0xFD7C_FF3F;
    /// USBSEL, FDCANSEL, I2S2SEL, I2S1SEL.
    pub const CCIPR2_PLAN: u32 = 0x0000_330F;
    /// CRS_CFGR without its reserved bits.
    pub const CRS_CFGR_ALL: u32 = 0xB7FF_FFFF;
}

pub use crate::{Mmio, Regs};

/// The step of [apply] that failed.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum Step {
    /// Leaving a running PLL for HSISYS before reprogramming (apply called on a chip not fresh from reset).
    BackToHsi,
    VosReady,
    HsiReady,
    /// HSE off before changing HSEBYP, which only changes while HSE is off (RM0444 §5.4.1).
    HseOff,
    HseReady,
    Hsi48Ready,
    PllReady,
    FlashLatency,
    Switch,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum Error {
    /// A ready flag didn't come within the bound.
    Timeout(Step),
}

/// A register that doesn't hold what the plan wrote.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub struct Mismatch {
    pub register: &'static str,
    pub addr: u32,
    pub mask: u32,
    pub want: u32,
    pub got: u32,
}

/// Polls of a ready flag before [Error::Timeout]. Each poll is a volatile read on the bus (a few cycles at 16 MHz
/// HSISYS); the slowest flag is HSE start-up at 2 ms typical for a crystal (DS13560 Table 42), so this is over 10x
/// that.
pub const POLLS: u32 = 200_000;

fn wait(r: &mut impl Regs, addr: u32, mask: u32, value: u32, step: Step) -> Result<(), Error> {
    for _ in 0..POLLS {
        if r.read(addr) & mask == value {
            return Ok(());
        }
    }
    Err(Error::Timeout(step))
}

fn modify(r: &mut impl Regs, addr: u32, clear: u32, set: u32) {
    let v = r.read(addr);
    r.write(addr, (v & !clear) | set);
}

/// Writes the plan, in the order of the module docs. Call it early in `main`, before anything that depends on the
/// clocks, on a chip fresh from reset (it also copes with a running PLL: it goes back to HSISYS first). Leaves the
/// PWR interface clock on (RCC_APBENR1 PWREN), as embassy-stm32 does.
pub fn apply(r: &mut impl Regs, p: &Plan) -> Result<(), Error> {
    use addr::*;
    use bits::*;

    // 0. Back to HSISYS with the PLL off when an earlier writer left them otherwise (RM0444 §5.2.4 steps 1-2)
    if r.read(RCC_CFGR) & CFGR_SWS != 0 {
        modify(r, RCC_CR, 0, CR_HSION);
        wait(r, RCC_CR, CR_HSIRDY, CR_HSIRDY, Step::BackToHsi)?;
        modify(r, RCC_CFGR, CFGR_SW, 0);
        wait(r, RCC_CFGR, CFGR_SWS, 0, Step::BackToHsi)?;
    }
    if r.read(RCC_CR) & CR_PLLON != 0 {
        modify(r, RCC_CR, CR_PLLON, 0);
        wait(r, RCC_CR, CR_PLLRDY, 0, Step::BackToHsi)?;
    }

    // 1. Range 2 to Range 1 before raising any clock: VOS, then VOSF clear (RM0444 §4.1.4)
    modify(r, RCC_APBENR1, 0, APBENR1_PWREN);
    let _ = r.read(RCC_APBENR1);
    let range1 = 0b01 << 9;
    let vos = r.read(PWR_CR1) & PWR_CR1_VOS;
    if p.pwr_cr1_vos == range1 && vos != range1 {
        modify(r, PWR_CR1, PWR_CR1_VOS, range1);
        wait(r, PWR_SR2, PWR_SR2_VOSF, 0, Step::VosReady)?;
    }

    // 2. Oscillators. HSEBYP only while HSE is off; it keeps its value through every reset but power-on (RM0444
    // §5.4.1), so a crystal plan after a bypass firmware, or the other way round, turns HSE off first.
    let osc = p.rcc_cr_osc;
    let cur = r.read(RCC_CR);
    if (cur ^ osc) & CR_HSEBYP != 0 {
        if cur & CR_HSEON != 0 {
            modify(r, RCC_CR, CR_HSEON, 0);
            wait(r, RCC_CR, CR_HSERDY, 0, Step::HseOff)?;
        }
        modify(r, RCC_CR, CR_HSEBYP, osc & CR_HSEBYP);
    }
    // HSIDIV takes the plan's value: SYSCLK is HSISYS here, and the plan's divider is what HSISYS is after
    modify(r, RCC_CR, CR_HSIDIV, osc & !CR_HSEBYP);
    if osc & CR_HSION != 0 {
        wait(r, RCC_CR, CR_HSIRDY, CR_HSIRDY, Step::HsiReady)?;
    }
    if osc & CR_HSEON != 0 {
        wait(r, RCC_CR, CR_HSERDY, CR_HSERDY, Step::HseReady)?;
    }
    if osc & CR_HSI48ON != 0 {
        wait(r, RCC_CR, CR_HSI48RDY, CR_HSI48RDY, Step::Hsi48Ready)?;
    }

    // 3. The PLL, configured while off (RM0444 §5.2.4)
    if p.rcc_cr_pll_on != 0 {
        r.write(RCC_PLLCFGR, p.pllcfgr);
        modify(r, RCC_CR, 0, CR_PLLON);
        wait(r, RCC_CR, CR_PLLRDY, CR_PLLRDY, Step::PllReady)?;
    }

    // 4. More wait states before the faster clock, read back until taken (RM0444 §3.3.4)
    let want_ws = p.flash_acr & ACR_LATENCY;
    if r.read(FLASH_ACR) & ACR_LATENCY < want_ws {
        modify(r, FLASH_ACR, ACR_LATENCY, want_ws);
        wait(r, FLASH_ACR, ACR_LATENCY, want_ws, Step::FlashLatency)?;
    }

    // 5. Switch, with the bus prescalers
    let sw = CFGR_SW | CFGR_HPRE | CFGR_PPRE;
    modify(r, RCC_CFGR, sw, p.cfgr & sw);
    wait(r, RCC_CFGR, CFGR_SWS, (p.cfgr & CFGR_SW) << 3, Step::Switch)?;

    // 6. The plan's FLASH_ACR: fewer wait states after a slower clock, PRFTEN, ICEN
    modify(r, FLASH_ACR, ACR_PLAN, p.flash_acr & ACR_PLAN);
    wait(r, FLASH_ACR, ACR_LATENCY, want_ws, Step::FlashLatency)?;

    // 7. Range 1 to Range 2 once SYSCLK and the wait states are down (RM0444 §4.1.4)
    if p.pwr_cr1_vos != range1 && vos != p.pwr_cr1_vos {
        modify(r, PWR_CR1, PWR_CR1_VOS, p.pwr_cr1_vos);
        wait(r, PWR_SR2, PWR_SR2_VOSF, 0, Step::VosReady)?;
    }

    // 8. Kernel muxes, every source running now
    modify(r, RCC_CCIPR, CCIPR_PLAN, p.ccipr & CCIPR_PLAN);
    modify(r, RCC_CCIPR2, CCIPR2_PLAN, p.ccipr2 & CCIPR2_PLAN);

    // 9. The CRS: CRS_CFGR is write-protected while CEN is set (RM0444 §6.7.2)
    if p.crs_cr != 0 {
        modify(r, RCC_APBENR1, 0, APBENR1_CRSEN);
        let _ = r.read(RCC_APBENR1);
        modify(r, CRS_CR, CRS_CR_CEN | CRS_CR_AUTOTRIMEN, 0);
        r.write(CRS_CFGR, p.crs_cfgr);
        r.write(CRS_CR, p.crs_cr);
    }

    // 10. Unused oscillators off
    if p.rcc_cr_off != 0 {
        modify(r, RCC_CR, p.rcc_cr_off, 0);
    }
    Ok(())
}

/// One planned field group of a register, as [verify] reads it back: the register reads `want` under `mask`
/// (mask 0: not part of the plan, skipped).
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct Check {
    pub register: &'static str,
    pub addr: u32,
    pub mask: u32,
    pub want: u32,
}

impl Plan {
    /// The read-back table of [verify], as a `const fn`: firmware that keeps it in a `static`
    /// (`static CHECKS: [Check; 10] = clocks::PLAN.checks();`) and calls [verify_checks] reads it from flash instead
    /// of building 160 bytes of it on the stack: 172 bytes less code on Donguru at opt-level z, 176 more at 3 (RCC-16).
    pub const fn checks(&self) -> [Check; 10] {
        use addr::*;
        use bits::*;
        let osc_mask = CR_HSION | CR_HSIDIV | CR_HSEON | CR_HSEBYP | CR_HSI48ON;
        let pll = if self.rcc_cr_pll_on != 0 {
            CR_PLLON | CR_PLLRDY
        } else {
            0
        };
        let crs = self.crs_cr != 0;
        const fn c(register: &'static str, addr: u32, mask: u32, want: u32) -> Check {
            Check {
                register,
                addr,
                mask,
                want: want & mask,
            }
        }
        [
            c("PWR_CR1", PWR_CR1, PWR_CR1_VOS, self.pwr_cr1_vos),
            c("FLASH_ACR", FLASH_ACR, ACR_PLAN, self.flash_acr),
            c("RCC_CR", RCC_CR, osc_mask, self.rcc_cr_osc),
            c("RCC_CR", RCC_CR, CR_PLLON | CR_PLLRDY, pll),
            c(
                "RCC_PLLCFGR",
                RCC_PLLCFGR,
                if pll != 0 { PLLCFGR_ALL } else { 0 },
                self.pllcfgr,
            ),
            c(
                "RCC_CFGR",
                RCC_CFGR,
                CFGR_SWS | CFGR_HPRE | CFGR_PPRE,
                ((self.cfgr & CFGR_SW) << 3) | (self.cfgr & (CFGR_HPRE | CFGR_PPRE)),
            ),
            c("RCC_CCIPR", RCC_CCIPR, CCIPR_PLAN, self.ccipr),
            c("RCC_CCIPR2", RCC_CCIPR2, CCIPR2_PLAN, self.ccipr2),
            c(
                "CRS_CFGR",
                CRS_CFGR,
                if crs { CRS_CFGR_ALL } else { 0 },
                self.crs_cfgr,
            ),
            c(
                "CRS_CR",
                CRS_CR,
                if crs {
                    CRS_CR_CEN | CRS_CR_AUTOTRIMEN
                } else {
                    0
                },
                self.crs_cr,
            ),
        ]
    }
}

/// Reads every register the plan wrote and compares the planned fields. Builds [Plan::checks] on the stack;
/// [verify_checks] on a `static` table is smaller.
pub fn verify(r: &mut impl Regs, p: &Plan) -> Result<(), Mismatch> {
    verify_checks(r, &p.checks())
}

/// [verify] on a table from [Plan::checks], typically a `static`.
pub fn verify_checks(r: &mut impl Regs, checks: &[Check]) -> Result<(), Mismatch> {
    for c in checks {
        if c.mask == 0 {
            continue;
        }
        let got = r.read(c.addr) & c.mask;
        if got != c.want {
            return Err(Mismatch {
                register: c.register,
                addr: c.addr,
                mask: c.mask,
                want: c.want,
                got,
            });
        }
    }
    Ok(())
}
