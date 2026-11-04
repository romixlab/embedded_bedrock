//! Register words for a checked config, built with stm32-metapac's register types and setters: the PAC places every
//! field (RM0468 §6.8 PWR, §4.9.1 FLASH_ACR, §8.7 RCC), the planner picks the values. The setters aren't `const fn`,
//! so this runs on the host and `clocks.rs` carries the words.
//!
//! Every word is the whole register: fields the planner doesn't model keep their reset value (0 for the kernel mux
//! registers, RM §8.7.18-8.7.21).

use stm32_metapac::flash::regs as flash;
use stm32_metapac::pwr::{regs as pwr, vals as pv};
use stm32_metapac::rcc::{regs as rcc, vals as rv};

use super::check::flash_wait_states;
use super::config::{Config, HseMode, Ker, Kernel, PllSource, SmpsLevel, Supply, SysSource, Vos};
use super::tree::Tree;

/// The registers `apply()` writes, and what it waits for.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct Regs {
    /// PWR_CR3 low byte (written once after POR; bits 7:6 kept at their reset value 01, RM §6.8.4).
    pub pwr_cr3: pwr::Cr3,
    /// PWR_D3CR with only VOS set (bits 15:14).
    pub pwr_d3cr_vos: pwr::D3cr,
    /// FLASH_ACR LATENCY and WRHIGHFREQ.
    pub flash_acr: flash::Acr,
    /// RCC_CR bits to set for the oscillators (ON bits, HSIDIV, HSEBYP), without the PLL ON bits.
    pub rcc_cr_osc: rcc::Cr,
    /// RCC_CR PLLxON bits of the PLLs in use.
    pub rcc_cr_pll_on: rcc::Cr,
    /// RCC_CR bits of the oscillators that stay off at the end (HSION when HSI isn't used).
    pub rcc_cr_off: rcc::Cr,
    pub pllckselr: rcc::Pllckselr,
    pub pllcfgr: rcc::Pllcfgr,
    pub plldivr: [rcc::Plldivr; 3],
    pub pllfracr: [rcc::Pllfracr; 3],
    pub cfgr: rcc::Cfgr,
    pub d1cfgr: rcc::D1cfgr,
    pub d2cfgr: rcc::D2cfgr,
    pub d3cfgr: rcc::D3cfgr,
    pub d1ccipr: rcc::D1ccipr,
    pub d2ccip1r: rcc::D2ccip1r,
    pub d2ccip2r: rcc::D2ccip2r,
    pub d3ccipr: rcc::D3ccipr,
}

/// PWR_CR3 low byte per supply (RM Table 30 p.237-238, §6.8.4 p.288); bit 6, reserved, at its reset value 1.
pub fn pwr_cr3(s: Supply) -> pwr::Cr3 {
    let mut r = pwr::Cr3(0x40);
    let lvl = |l: SmpsLevel| match l {
        SmpsLevel::V1_8 => pv::Sdlevel::V1_8,
        SmpsLevel::V2_5 => pv::Sdlevel::V2_5,
    };
    // (BYPASS, LDOEN, SDEN, SDEXTHP, SDLEVEL)
    let (bypass, ldo, smps, ext, level) = match s {
        Supply::Default => (false, true, true, false, None),
        Supply::Ldo => (false, true, false, false, None),
        Supply::DirectSmps => (false, false, true, false, None),
        Supply::SmpsLdo(l) => (false, true, true, false, Some(lvl(l))),
        Supply::SmpsExternalLdo(l) => (false, true, true, true, Some(lvl(l))),
        Supply::SmpsExternalBypass(l) => (true, false, true, true, Some(lvl(l))),
        Supply::Bypass => (true, false, false, false, None),
    };
    r.set_bypass(bypass);
    r.set_ldoen(ldo);
    r.set_sden(smps);
    r.set_sdexthp(ext);
    if let Some(l) = level {
        r.set_sdlevel(l);
    }
    r
}

/// PWR_D3CR with VOS (RM §6.8.6 p.291).
pub fn vos_bits(v: Vos) -> pwr::D3cr {
    let mut r = pwr::D3cr(0);
    r.set_vos(match v {
        Vos::Vos0 => pv::Vos::SCALE0,
        Vos::Vos1 => pv::Vos::SCALE1,
        Vos::Vos2 => pv::Vos::SCALE2,
        Vos::Vos3 => pv::Vos::SCALE3,
    });
    r
}

/// A kernel mux: its config key, the selection, and each choice with its field value.
pub type Mux = (&'static str, Ker, &'static [(Ker, u8)]);

/// The 15 kernel muxes (RM §8.7.18-8.7.21). One table for the checker (a selection not listed is refused) and the
/// encoder.
pub fn muxes(k: &Kernel) -> [Mux; 15] {
    use Ker::*;
    // FMCSEL and OCTOSPISEL; 11 of FDCANSEL and CKPERSEL is "reserved, the kernel clock is disabled"
    const FMC: &[(Ker, u8)] = &[(Bus, 0), (Pll1Q, 1), (Pll2R, 2), (PerCk, 3)];
    const SPI45: &[(Ker, u8)] = &[
        (Bus, 0),
        (Pll2Q, 1),
        (Pll3Q, 2),
        (HsiKer, 3),
        (CsiKer, 4),
        (Hse, 5),
    ];
    const USART: &[(Ker, u8)] = &[
        (Bus, 0),
        (Pll2Q, 1),
        (Pll3Q, 2),
        (HsiKer, 3),
        (CsiKer, 4),
        (Lse, 5),
    ];
    const I2C: &[(Ker, u8)] = &[(Bus, 0), (Pll3R, 1), (HsiKer, 2), (CsiKer, 3)];
    [
        (
            "kernel.per_ck",
            k.per_ck,
            &[(HsiKer, 0), (CsiKer, 1), (Hse, 2), (Disabled, 3)],
        ),
        ("kernel.sdmmc", k.sdmmc, &[(Pll1Q, 0), (Pll2R, 1)]),
        ("kernel.octospi", k.octospi, FMC),
        ("kernel.fmc", k.fmc, FMC),
        (
            "kernel.fdcan",
            k.fdcan,
            &[(Hse, 0), (Pll1Q, 1), (Pll2Q, 2), (Disabled, 3)],
        ),
        ("kernel.spi45", k.spi45, SPI45),
        // I2S_CKIN (3) not modelled
        (
            "kernel.spi123",
            k.spi123,
            &[(Pll1Q, 0), (Pll2P, 1), (Pll3P, 2), (PerCk, 4)],
        ),
        (
            "kernel.usb",
            k.usb,
            &[(Disabled, 0), (Pll1Q, 1), (Pll3Q, 2), (Hsi48, 3)],
        ),
        ("kernel.i2c1235", k.i2c1235, I2C),
        ("kernel.usart16910", k.usart16910, USART),
        ("kernel.usart234578", k.usart234578, USART),
        ("kernel.spi6", k.spi6, SPI45),
        ("kernel.adc", k.adc, &[(Pll2P, 0), (Pll3R, 1), (PerCk, 2)]),
        ("kernel.i2c4", k.i2c4, I2C),
        ("kernel.lpuart1", k.lpuart1, USART),
    ]
}

/// The field value of mux `key`'s selection; the checker refused a selection the mux doesn't have.
fn mux(k: &Kernel, key: &str) -> u8 {
    muxes(k)
        .iter()
        .find(|(name, _, _)| *name == key)
        .and_then(|(_, sel, ok)| ok.iter().find(|(c, _)| c == sel))
        .map(|(_, v)| *v)
        .expect("checked mux choice")
}

/// Encodes a config the checker passed without errors.
pub fn encode(cfg: &Config, t: &Tree) -> Regs {
    let ws = flash_wait_states(t.hclk.expect("checked"), cfg.vos).expect("checked");
    let mut flash_acr = flash::Acr(0);
    flash_acr.set_latency(ws);
    // WRHIGHFREQ follows the wait-state band (RM Table 16)
    flash_acr.set_wrhighfreq(ws);

    let (mut osc, mut off) = (rcc::Cr(0), rcc::Cr(0));
    match cfg.hsi_div {
        Some(d) => {
            osc.set_hsion(true);
            osc.set_hsidiv(d.bits());
        }
        None => off.set_hsion(true),
    }
    osc.set_csion(cfg.csi);
    osc.set_hsi48on(cfg.hsi48);
    if let Some(h) = cfg.hse {
        osc.set_hseon(true);
        osc.set_hsebyp(h.mode == HseMode::Bypass);
    }

    // DIVMx 0 (prescaler off) for an unused PLL; an unused output has its enable bit and divider at 0 (RM §8.5.5
    // p.328)
    let mut ckselr = rcc::Pllckselr(0);
    ckselr.set_pllsrc(match cfg.pll_source {
        PllSource::Hsi => rv::Pllsrc::HSI,
        PllSource::Csi => rv::Pllsrc::CSI,
        PllSource::Hse => rv::Pllsrc::HSE,
    });
    let (mut pll_on, mut pllcfgr) = (rcc::Cr(0), rcc::Pllcfgr(0));
    let mut plldivr = [rcc::Plldivr(0); 3];
    let mut pllfracr = [rcc::Pllfracr(0); 3];
    for (i, p) in cfg.pll.iter().enumerate() {
        let Some(p) = p else { continue };
        let o = t.pll[i].expect("checked");
        ckselr.set_divm(i, rv::Pllm::from_bits(p.m.get()));
        pll_on.set_pllon(i, true);
        pllcfgr.set_pllfracen(i, p.fracn.is_some());
        pllcfgr.set_pllvcosel(
            i,
            if o.wide {
                rv::Pllvcosel::WIDE_VCO
            } else {
                rv::Pllvcosel::MEDIUM_VCO
            },
        );
        pllcfgr.set_pllrge(i, o.rge);
        pllcfgr.set_divpen(i, p.p.is_some());
        pllcfgr.set_divqen(i, p.q.is_some());
        pllcfgr.set_divren(i, p.r.is_some());
        // each divider field is the ratio - 1
        let d = &mut plldivr[i];
        d.set_plln(rv::Plln::from_bits(p.n.get() - 1));
        let div =
            |v: Option<super::config::Divpqr>| rv::Plldiv::from_bits(v.map_or(0, |v| v.get() - 1));
        d.set_pllp(div(p.p));
        d.set_pllq(div(p.q));
        d.set_pllr(div(p.r));
        pllfracr[i].set_fracn(p.fracn.map_or(0, |f| f.get()));
    }

    let s = &cfg.sys;
    let mut cfgr = rcc::Cfgr(0);
    cfgr.set_sw(match s.source {
        SysSource::Hsi => rv::Sw::HSI,
        SysSource::Csi => rv::Sw::CSI,
        SysSource::Hse => rv::Sw::HSE,
        SysSource::Pll1P => rv::Sw::PLL1_P,
    });
    cfgr.set_timpre(if s.timpre {
        rv::Timpre::DEFAULT_X4
    } else {
        rv::Timpre::DEFAULT_X2
    });
    let mut d1cfgr = rcc::D1cfgr(0);
    d1cfgr.set_d1cpre(s.d1cpre.bits());
    d1cfgr.set_hpre(s.hpre.bits());
    d1cfgr.set_d1ppre(s.d1ppre.bits());
    let mut d2cfgr = rcc::D2cfgr(0);
    d2cfgr.set_d2ppre1(s.d2ppre1.bits());
    d2cfgr.set_d2ppre2(s.d2ppre2.bits());
    let mut d3cfgr = rcc::D3cfgr(0);
    d3cfgr.set_d3ppre(s.d3ppre.bits());

    let k = &cfg.kernel;
    let m = |key| mux(k, key);
    let mut d1ccipr = rcc::D1ccipr(0);
    d1ccipr.set_fmcsel(rv::Fmcsel::from_bits(m("kernel.fmc")));
    d1ccipr.set_octospisel(rv::Fmcsel::from_bits(m("kernel.octospi")));
    d1ccipr.set_sdmmcsel(rv::Sdmmcsel::from_bits(m("kernel.sdmmc")));
    d1ccipr.set_persel(rv::Persel::from_bits(m("kernel.per_ck")));
    let mut d2ccip1r = rcc::D2ccip1r(0);
    d2ccip1r.set_spi123sel(rv::Saisel::from_bits(m("kernel.spi123")));
    d2ccip1r.set_spi45sel(rv::Spi45sel::from_bits(m("kernel.spi45")));
    d2ccip1r.set_fdcansel(rv::Fdcansel::from_bits(m("kernel.fdcan")));
    let mut d2ccip2r = rcc::D2ccip2r(0);
    d2ccip2r.set_usart234578sel(rv::Usart234578sel::from_bits(m("kernel.usart234578")));
    d2ccip2r.set_usart16910sel(rv::Usart16910sel::from_bits(m("kernel.usart16910")));
    d2ccip2r.set_i2c1235sel(rv::I2c1235sel::from_bits(m("kernel.i2c1235")));
    d2ccip2r.set_usbsel(rv::Usbsel::from_bits(m("kernel.usb")));
    let mut d3ccipr = rcc::D3ccipr(0);
    d3ccipr.set_lpuart1sel(rv::Lpuartsel::from_bits(m("kernel.lpuart1")));
    d3ccipr.set_i2c4sel(rv::I2c4sel::from_bits(m("kernel.i2c4")));
    d3ccipr.set_adcsel(rv::Adcsel::from_bits(m("kernel.adc")));
    d3ccipr.set_spi6sel(rv::Spi6sel::from_bits(m("kernel.spi6")));

    Regs {
        pwr_cr3: pwr_cr3(cfg.supply),
        pwr_d3cr_vos: vos_bits(cfg.vos),
        flash_acr,
        rcc_cr_osc: osc,
        rcc_cr_pll_on: pll_on,
        rcc_cr_off: off,
        pllckselr: ckselr,
        pllcfgr,
        plldivr,
        pllfracr,
        cfgr,
        d1cfgr,
        d2cfgr,
        d3cfgr,
        d1ccipr,
        d2ccip1r,
        d2ccip2r,
        d3ccipr,
    }
}
