//! The RM0468 family: STM32H723/733, H725/735, H730.
//!
//! [apply] follows RM0468 Rev 3:
//! 1. PWR_CR3 supply configuration, written once per power-on (§6.8.4, §6.4.1); then ACTVOSRDY.
//! 2. PWR_D3CR VOS, then VOSRDY; for VOS0 also ACTVOS == VOS with ACTVOSRDY (§6.8.6).
//! 3. Oscillators: HSEBYP before HSEON, then their ready flags (§8.5.2, §8.7.2).
//! 4. PLLs in the Figure 48 order: PLLCKSELR, PLLxFRACR, PLLCFGR, PLLxDIVR, PLLxON, PLLxRDY (§8.5.5).
//! 5. FLASH_ACR wait states for the new AXI clock, read back (§4.3.8).
//! 6. D1CFGR / D2CFGR / D3CFGR prescalers, read back (§8.7.7 caution).
//! 7. CFGR SW (and TIMPRE), then SWS (§8.7.6).
//! 8. Kernel muxes last, with the old and the new source running (§8.5.8 p.352).
//! 9. Oscillators the plan doesn't use off (HSI).

/// The register words of a plan, as `bedrock_clocks` emits them in `clocks.rs` (`PLAN`).
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub struct Plan {
    /// PWR_CR3 low byte (supply configuration, RM0468 Table 30).
    pub pwr_cr3: u32,
    /// PWR_D3CR VOS field in place (bits 15:14).
    pub pwr_d3cr_vos: u32,
    pub flash_acr: u32,
    /// RCC_CR oscillator bits to set (ON bits, HSIDIV, HSEBYP).
    pub rcc_cr_osc: u32,
    /// RCC_CR PLLxON bits.
    pub rcc_cr_pll_on: u32,
    /// RCC_CR oscillator ON bits to clear at the end.
    pub rcc_cr_off: u32,
    pub pllckselr: u32,
    pub pllcfgr: u32,
    pub plldivr: [u32; 3],
    pub pllfracr: [u32; 3],
    pub cfgr: u32,
    pub d1cfgr: u32,
    pub d2cfgr: u32,
    pub d3cfgr: u32,
    pub d1ccipr: u32,
    pub d2ccip1r: u32,
    pub d2ccip2r: u32,
    pub d3ccipr: u32,
}

/// A PLL's FRACN steering range from the plan, and its step.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub struct FracnRange {
    /// 1, 2 or 3.
    pub pll: u8,
    /// DIVN as a ratio.
    pub n: u16,
    pub nominal: u16,
    pub min: u16,
    pub max: u16,
    /// One FRACN step moves the VCO by num/den Hz (ref_ck / 8192).
    pub vco_step_hz_num: u64,
    pub vco_step_hz_den: u64,
}

impl FracnRange {
    /// One step relative to the VCO at `fracn`, in parts per 10¹²: 10¹² / (N·8192 + FRACN), truncated (lossy, for
    /// a trim loop that works in ppb or finer).
    pub const fn step_ppt_at(&self, fracn: u16) -> u64 {
        1_000_000_000_000 / (self.n as u64 * 8192 + fracn as u64)
    }
}

/// RCC, PWR and FLASH register addresses (RM0468 §2.3 memory map; held to stm32-metapac in a test).
pub mod addr {
    pub const RCC: u32 = 0x5802_4400;
    pub const PWR: u32 = 0x5802_4800;
    pub const FLASH: u32 = 0x5200_2000;

    pub const RCC_CR: u32 = RCC;
    pub const RCC_CFGR: u32 = RCC + 0x10;
    pub const RCC_D1CFGR: u32 = RCC + 0x18;
    pub const RCC_D2CFGR: u32 = RCC + 0x1C;
    pub const RCC_D3CFGR: u32 = RCC + 0x20;
    pub const RCC_PLLCKSELR: u32 = RCC + 0x28;
    pub const RCC_PLLCFGR: u32 = RCC + 0x2C;
    /// PLLxDIVR at 0x30 + 8·(x-1), PLLxFRACR at 0x34 + 8·(x-1).
    pub const fn rcc_plldivr(i: usize) -> u32 {
        RCC + 0x30 + 8 * i as u32
    }
    pub const fn rcc_pllfracr(i: usize) -> u32 {
        RCC + 0x34 + 8 * i as u32
    }
    pub const RCC_D1CCIPR: u32 = RCC + 0x4C;
    pub const RCC_D2CCIP1R: u32 = RCC + 0x50;
    pub const RCC_D2CCIP2R: u32 = RCC + 0x54;
    pub const RCC_D3CCIPR: u32 = RCC + 0x58;

    pub const PWR_CSR1: u32 = PWR + 0x04;
    pub const PWR_CR3: u32 = PWR + 0x0C;
    pub const PWR_D3CR: u32 = PWR + 0x18;

    pub const FLASH_ACR: u32 = FLASH;
}

/// RCC_CR bits (RM0468 §8.7.2).
mod cr {
    pub const HSIRDY: u32 = 1 << 2;
    pub const HSIDIVF: u32 = 1 << 5;
    pub const CSION: u32 = 1 << 7;
    pub const CSIRDY: u32 = 1 << 8;
    pub const HSI48ON: u32 = 1 << 12;
    pub const HSI48RDY: u32 = 1 << 13;
    pub const HSEON: u32 = 1 << 16;
    pub const HSERDY: u32 = 1 << 17;
    pub const HSEBYP: u32 = 1 << 18;
    pub const HSION: u32 = 1;
    pub const fn pll_on(i: usize) -> u32 {
        1 << (24 + 2 * i)
    }
    pub const fn pll_rdy(i: usize) -> u32 {
        1 << (25 + 2 * i)
    }
}

/// Register access. [Mmio] on the chip; a simulated chip in the host tests.
pub trait Regs {
    fn read(&mut self, addr: u32) -> u32;
    fn write(&mut self, addr: u32, value: u32);
}

/// The chip's own registers.
pub struct Mmio;

impl Regs for Mmio {
    #[inline(always)]
    fn read(&mut self, addr: u32) -> u32 {
        // SAFETY: only the RCC / PWR / FLASH register addresses of `addr` reach here
        unsafe { core::ptr::read_volatile(addr as *const u32) }
    }
    #[inline(always)]
    fn write(&mut self, addr: u32, value: u32) {
        // SAFETY: as above
        unsafe { core::ptr::write_volatile(addr as *mut u32, value) }
    }
}

/// The step of [apply] that failed.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum Step {
    SupplyReady,
    VosReady,
    HsiReady,
    CsiReady,
    Hsi48Ready,
    HseReady,
    /// 0, 1, 2 for PLL1..3.
    PllReady(u8),
    FlashLatency,
    Prescalers,
    Switch,
    /// Leaving a running PLL for HSI before reprogramming (apply called on a chip not fresh from reset).
    BackToHsi,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum Error {
    /// PWR_CR3 was already written since power-on with another supply configuration (a bootloader, or an earlier
    /// firmware): it is written once per power-on (RM0468 §6.8.4), so only a power cycle changes it.
    SupplyLocked { found: u8, wanted: u8 },
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

/// Polls of a ready flag before [Error::Timeout]. Each poll is a volatile read on the APB (a few cycles at 64 MHz
/// HSI); the slowest flag is HSE start-up at 2 ms typical for a crystal (DS13311 Table 37), so this is over 10x that.
pub const POLLS: u32 = 2_000_000;

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

/// PWR_CR3 bits the supply configuration owns (SDLEVEL, SDEXTHP, SDEN, LDOEN, BYPASS: RM0468 §6.8.4).
const CR3_SUPPLY: u32 = 0x3F;
const ACTVOSRDY: u32 = 1 << 13;
const VOSRDY: u32 = 1 << 13;
const VOS_MASK: u32 = 0b11 << 14;
const SW_MASK: u32 = 0b111;
const TIMPRE: u32 = 1 << 15;

/// Writes the plan, in the order of the module docs. Call it early in `main`, before anything that depends on the
/// clocks, on a chip fresh from reset (it also copes with a running PLL: it goes back to HSI first).
pub fn apply(r: &mut impl Regs, p: &Plan) -> Result<(), Error> {
    use addr::*;

    // 0. Not fresh from reset (apply twice, or after a bootloader that left a PLL running): back to HSI and
    // PLLs off, since DIVM, PLLSRC and the dividers can't change while a PLL runs (RM §8.5.5 p.327-328).
    if r.read(RCC_CFGR) & (SW_MASK << 3) != 0 {
        modify(r, RCC_CR, 0, cr::HSION);
        wait(r, RCC_CR, cr::HSIRDY, cr::HSIRDY, Step::BackToHsi)?;
        modify(r, RCC_CFGR, SW_MASK, 0);
        wait(r, RCC_CFGR, SW_MASK << 3, 0, Step::BackToHsi)?;
    }
    let all_pll_on = cr::pll_on(0) | cr::pll_on(1) | cr::pll_on(2);
    if r.read(RCC_CR) & all_pll_on != 0 {
        modify(r, RCC_CR, all_pll_on, 0);
        let all_rdy = cr::pll_rdy(0) | cr::pll_rdy(1) | cr::pll_rdy(2);
        wait(r, RCC_CR, all_rdy, 0, Step::BackToHsi)?;
    }

    // 1. Supply configuration: write once per power-on, then read back. A write the lock ignores leaves the old
    // value, which is the clear error rather than a hang on ACTVOSRDY.
    let cr3 = r.read(PWR_CR3);
    let wanted = p.pwr_cr3 & CR3_SUPPLY;
    if cr3 & CR3_SUPPLY != wanted {
        r.write(PWR_CR3, (cr3 & !CR3_SUPPLY) | wanted);
        let now = r.read(PWR_CR3) & CR3_SUPPLY;
        if now != wanted {
            return Err(Error::SupplyLocked {
                found: now as u8,
                wanted: wanted as u8,
            });
        }
    }
    wait(r, PWR_CSR1, ACTVOSRDY, ACTVOSRDY, Step::SupplyReady)?;

    // 2. VOS up before the clocks go up
    modify(r, PWR_D3CR, VOS_MASK, p.pwr_d3cr_vos);
    wait(r, PWR_D3CR, VOSRDY, VOSRDY, Step::VosReady)?;
    if p.pwr_d3cr_vos == 0 {
        // VOS0: ACTVOS must equal VOS with ACTVOSRDY before raising the clock above the VOS1 maximum
        wait(r, PWR_CSR1, VOS_MASK | ACTVOSRDY, ACTVOSRDY, Step::VosReady)?;
    }

    // 3. Oscillators. HSEBYP only while HSE is off (RM §8.7.2), so before HSEON.
    let osc = p.rcc_cr_osc;
    modify(r, RCC_CR, 0, osc & cr::HSEBYP);
    modify(r, RCC_CR, 0, osc & !cr::HSEBYP);
    if osc & cr::HSION != 0 {
        wait(
            r,
            RCC_CR,
            cr::HSIRDY | cr::HSIDIVF,
            cr::HSIRDY | cr::HSIDIVF,
            Step::HsiReady,
        )?;
    }
    if osc & cr::CSION != 0 {
        wait(r, RCC_CR, cr::CSIRDY, cr::CSIRDY, Step::CsiReady)?;
    }
    if osc & cr::HSI48ON != 0 {
        wait(r, RCC_CR, cr::HSI48RDY, cr::HSI48RDY, Step::Hsi48Ready)?;
    }
    if osc & cr::HSEON != 0 {
        wait(r, RCC_CR, cr::HSERDY, cr::HSERDY, Step::HseReady)?;
    }

    // 4. PLLs, Figure 48 order
    r.write(RCC_PLLCKSELR, p.pllckselr);
    for i in 0..3 {
        r.write(rcc_pllfracr(i), p.pllfracr[i]);
    }
    r.write(RCC_PLLCFGR, p.pllcfgr);
    for i in 0..3 {
        r.write(rcc_plldivr(i), p.plldivr[i]);
    }
    modify(r, RCC_CR, 0, p.rcc_cr_pll_on);
    for i in 0..3 {
        if p.rcc_cr_pll_on & cr::pll_on(i) != 0 {
            wait(
                r,
                RCC_CR,
                cr::pll_rdy(i),
                cr::pll_rdy(i),
                Step::PllReady(i as u8),
            )?;
        }
    }

    // 5. Flash wait states for the new AXI clock, before the switch
    r.write(FLASH_ACR, p.flash_acr);
    wait(r, FLASH_ACR, 0x3F, p.flash_acr & 0x3F, Step::FlashLatency)?;

    // 6. Prescalers, read back
    r.write(RCC_D1CFGR, p.d1cfgr);
    r.write(RCC_D2CFGR, p.d2cfgr);
    r.write(RCC_D3CFGR, p.d3cfgr);
    wait(r, RCC_D1CFGR, 0xF7F, p.d1cfgr & 0xF7F, Step::Prescalers)?;
    wait(r, RCC_D2CFGR, 0x770, p.d2cfgr & 0x770, Step::Prescalers)?;
    wait(r, RCC_D3CFGR, 0x70, p.d3cfgr & 0x70, Step::Prescalers)?;

    // 7. Switch
    modify(r, RCC_CFGR, SW_MASK | TIMPRE, p.cfgr & (SW_MASK | TIMPRE));
    wait(
        r,
        RCC_CFGR,
        SW_MASK << 3,
        (p.cfgr & SW_MASK) << 3,
        Step::Switch,
    )?;

    // 8. Kernel muxes, every source running now
    r.write(RCC_D1CCIPR, p.d1ccipr);
    r.write(RCC_D2CCIP1R, p.d2ccip1r);
    r.write(RCC_D2CCIP2R, p.d2ccip2r);
    r.write(RCC_D3CCIPR, p.d3ccipr);

    // 9. Unused oscillators off
    if p.rcc_cr_off != 0 {
        modify(r, RCC_CR, p.rcc_cr_off, 0);
    }
    Ok(())
}

/// Reads every register the plan wrote and compares the planned fields.
pub fn verify(r: &mut impl Regs, p: &Plan) -> Result<(), Mismatch> {
    use addr::*;
    let pll_rdy = (0..3)
        .filter(|&i| p.rcc_cr_pll_on & cr::pll_on(i) != 0)
        .fold(0, |m, i| m | cr::pll_rdy(i));
    let osc_mask = cr::HSEON | cr::HSEBYP | cr::CSION | cr::HSI48ON | (0b11 << 3);
    let checks: [(&'static str, u32, u32, u32); 21] = [
        ("PWR_CR3", PWR_CR3, CR3_SUPPLY, p.pwr_cr3),
        ("PWR_D3CR", PWR_D3CR, VOS_MASK, p.pwr_d3cr_vos),
        ("FLASH_ACR", FLASH_ACR, 0x3F, p.flash_acr),
        ("RCC_CR", RCC_CR, osc_mask, p.rcc_cr_osc & osc_mask),
        (
            "RCC_CR",
            RCC_CR,
            p.rcc_cr_pll_on | pll_rdy,
            p.rcc_cr_pll_on | pll_rdy,
        ),
        ("RCC_CR", RCC_CR, p.rcc_cr_off, 0),
        ("RCC_PLLCKSELR", RCC_PLLCKSELR, 0x03F3_F3F3, p.pllckselr),
        ("RCC_PLLCFGR", RCC_PLLCFGR, 0x01FF_0FFF, p.pllcfgr),
        ("RCC_PLL1DIVR", rcc_plldivr(0), 0x7F7F_FFFF, p.plldivr[0]),
        ("RCC_PLL2DIVR", rcc_plldivr(1), 0x7F7F_FFFF, p.plldivr[1]),
        ("RCC_PLL3DIVR", rcc_plldivr(2), 0x7F7F_FFFF, p.plldivr[2]),
        ("RCC_PLL1FRACR", rcc_pllfracr(0), 0xFFF8, p.pllfracr[0]),
        ("RCC_PLL2FRACR", rcc_pllfracr(1), 0xFFF8, p.pllfracr[1]),
        ("RCC_PLL3FRACR", rcc_pllfracr(2), 0xFFF8, p.pllfracr[2]),
        (
            "RCC_CFGR",
            RCC_CFGR,
            (SW_MASK << 3) | TIMPRE,
            ((p.cfgr & SW_MASK) << 3) | (p.cfgr & TIMPRE),
        ),
        ("RCC_D1CFGR", RCC_D1CFGR, 0xF7F, p.d1cfgr),
        ("RCC_D2CFGR", RCC_D2CFGR, 0x770, p.d2cfgr),
        ("RCC_D3CFGR", RCC_D3CFGR, 0x70, p.d3cfgr),
        ("RCC_D1CCIPR", RCC_D1CCIPR, 0x3001_0033, p.d1ccipr),
        ("RCC_D2CCIP1R", RCC_D2CCIP1R, 0x3007_7000, p.d2ccip1r),
        ("RCC_D2CCIP2R", RCC_D2CCIP2R, 0x0030_303F, p.d2ccip2r),
    ];
    for (register, addr, mask, want) in checks {
        let got = r.read(addr);
        if got & mask != want & mask {
            return Err(Mismatch {
                register,
                addr,
                mask,
                want: want & mask,
                got: got & mask,
            });
        }
    }
    let got = r.read(RCC_D3CCIPR);
    let mask = 0x7003_0307;
    if got & mask != p.d3ccipr & mask {
        return Err(Mismatch {
            register: "RCC_D3CCIPR",
            addr: RCC_D3CCIPR,
            mask,
            want: p.d3ccipr & mask,
            got: got & mask,
        });
    }
    Ok(())
}

/// A FRACN value outside the plan's steering range.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub struct OutOfRange {
    pub fracn: u16,
    pub min: u16,
    pub max: u16,
}

/// Sets a running PLL's FRACN on the fly (RM0468 §8.5.5 p.329, §8.7.13): PLLxFRACEN 0, write FRACN, PLLxFRACEN 1.
/// Refuses a value outside the plan's range, which the planner checked against every limit at both ends.
pub fn set_fracn(r: &mut impl Regs, range: &FracnRange, fracn: u16) -> Result<(), OutOfRange> {
    if fracn < range.min || fracn > range.max {
        return Err(OutOfRange {
            fracn,
            min: range.min,
            max: range.max,
        });
    }
    let i = (range.pll as usize).saturating_sub(1).min(2);
    let fracen = 1 << (4 * i);
    modify(r, addr::RCC_PLLCFGR, fracen, 0);
    r.write(addr::rcc_pllfracr(i), (fracn as u32) << 3);
    modify(r, addr::RCC_PLLCFGR, 0, fracen);
    Ok(())
}
