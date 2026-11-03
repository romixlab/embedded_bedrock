//! Register words for a checked config. Field positions are from RM0468 §6.8 (PWR), §4.9.1 (FLASH_ACR) and §8.7
//! (RCC); a test holds them to stm32-metapac's register types.
//!
//! Every word is the whole register: fields the planner doesn't model keep their reset value (0 for the kernel mux
//! registers, RM §8.7.18-8.7.21).

use super::check::flash_wait_states;
use super::config::{Config, Ker, PllSource, SmpsLevel, Supply, SysSource, Vos};
use super::tree::Tree;

/// The register words `apply()` writes, and what it waits for.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
pub struct Regs {
    /// PWR_CR3 low byte (written once after POR; bits 7:6 kept at their reset value 01, RM §6.8.4).
    pub pwr_cr3: u32,
    /// PWR_D3CR VOS field in place (bits 15:14).
    pub pwr_d3cr_vos: u32,
    /// FLASH_ACR LATENCY and WRHIGHFREQ.
    pub flash_acr: u32,
    /// RCC_CR bits to set for the oscillators (ON bits, HSIDIV, HSEBYP), without the PLL ON bits.
    pub rcc_cr_osc: u32,
    /// RCC_CR PLLxON bits of the PLLs in use.
    pub rcc_cr_pll_on: u32,
    /// RCC_CR bits of the oscillators that stay off at the end (HSION when HSI isn't used).
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

/// PWR_CR3 low byte per supply (RM Table 30 p.237-238, bit layout §6.8.4 p.288): BYPASS 0, LDOEN 1, SDEN 2,
/// SDEXTHP 3, SDLEVEL 5:4, bit 6 reserved at reset value 1.
pub fn pwr_cr3(s: Supply) -> u32 {
    let lvl = |l: SmpsLevel| match l {
        SmpsLevel::V1_8 => 0b01 << 4,
        SmpsLevel::V2_5 => 0b10 << 4,
    };
    0x40 | match s {
        Supply::Default => 0b0110,
        Supply::Ldo => 0b0010,
        Supply::DirectSmps => 0b0100,
        Supply::SmpsLdo(l) => lvl(l) | 0b0110,
        Supply::SmpsExternalLdo(l) => lvl(l) | 0b1110,
        Supply::SmpsExternalBypass(l) => lvl(l) | 0b1101,
        Supply::Bypass => 0b0001,
    }
}

/// PWR_D3CR VOS encoding (RM §6.8.6 p.291): 00 VOS0, 11 VOS1, 10 VOS2, 01 VOS3.
pub const fn vos_bits(v: Vos) -> u32 {
    (match v {
        Vos::Vos0 => 0b00,
        Vos::Vos1 => 0b11,
        Vos::Vos2 => 0b10,
        Vos::Vos3 => 0b01,
    }) << 14
}

/// D1CPRE / HPRE field for a ratio (RM §8.7.7): 0 = ÷1, 8 = ÷2 ... 15 = ÷512, no ÷32.
pub fn hpre_bits(r: u16) -> u32 {
    match r {
        1 => 0,
        2 => 8,
        4 => 9,
        8 => 10,
        16 => 11,
        64 => 12,
        128 => 13,
        256 => 14,
        512 => 15,
        _ => unreachable!("checked"),
    }
}

/// APB prescaler field for a ratio (RM §8.7.7-8.7.9): 0 = ÷1, 4 = ÷2 ... 7 = ÷16.
pub fn ppre_bits(r: u8) -> u32 {
    match r {
        1 => 0,
        2 => 4,
        4 => 5,
        8 => 6,
        16 => 7,
        _ => unreachable!("checked"),
    }
}

fn mux(k: Ker, table: &[(Ker, u32)]) -> u32 {
    table
        .iter()
        .find(|(c, _)| *c == k)
        .map(|(_, v)| *v)
        .expect("checked mux choice")
}

/// Encodes a config the checker passed without errors.
pub fn encode(cfg: &Config, t: &Tree) -> Regs {
    let mut r = Regs {
        pwr_cr3: pwr_cr3(cfg.supply),
        pwr_d3cr_vos: vos_bits(cfg.vos),
        ..Default::default()
    };

    let ws = flash_wait_states(t.hclk.expect("checked"), cfg.vos).expect("checked") as u32;
    // WRHIGHFREQ follows the wait-state band (RM Table 16)
    r.flash_acr = ws | (ws << 4);

    // RCC_CR (RM §8.7.2): HSION 0, HSIDIV 4:3, CSION 7, HSI48ON 12, HSEON 16, HSEBYP 18, PLLxON 24/26/28
    if let Some(d) = cfg.hsi_div {
        r.rcc_cr_osc |= 1
            | (match d {
                1 => 0,
                2 => 1,
                4 => 2,
                _ => 3,
            } << 3);
    } else {
        r.rcc_cr_off |= 1;
    }
    if cfg.csi {
        r.rcc_cr_osc |= 1 << 7;
    }
    if cfg.hsi48 {
        r.rcc_cr_osc |= 1 << 12;
    }
    if let Some(h) = cfg.hse {
        r.rcc_cr_osc |= 1 << 16;
        if h.mode == super::config::HseMode::Bypass {
            r.rcc_cr_osc |= 1 << 18;
        }
    }

    // PLLCKSELR (RM §8.7.10): PLLSRC 1:0, DIVM1 9:4, DIVM2 17:12, DIVM3 25:20; DIVM 0 for an unused PLL
    r.pllckselr = match cfg.pll_source {
        PllSource::Hsi => 0,
        PllSource::Csi => 1,
        PllSource::Hse => 2,
    };
    for (i, p) in cfg.pll.iter().enumerate() {
        let Some(p) = p else { continue };
        let o = t.pll[i].expect("checked");
        r.pllckselr |= (p.m as u32) << [4, 12, 20][i];
        r.rcc_cr_pll_on |= 1 << (24 + 2 * i);
        // PLLCFGR (RM §8.7.11): FRACEN 4i, VCOSEL 4i+1 (1 = medium), RGE 4i+3:4i+2, DIVxEN 16+3i..18+3i
        let base = 4 * i;
        if p.fracn.is_some() {
            r.pllcfgr |= 1 << base;
        }
        if !o.wide {
            r.pllcfgr |= 1 << (base + 1);
        }
        r.pllcfgr |= (o.rge as u32) << (base + 2);
        // PLLxDIVR (RM §8.7.12): DIVN 8:0, DIVP 15:9, DIVQ 22:16, DIVR 30:24, each the ratio - 1; an unused output
        // has its enable bit and divider at 0 (RM §8.5.5 p.328)
        let mut d = (p.n as u32 - 1) & 0x1FF;
        for (j, (v, shift)) in [(p.p, 9), (p.q, 16), (p.r, 24)].into_iter().enumerate() {
            if let Some(v) = v {
                d |= ((v as u32 - 1) & 0x7F) << shift;
                r.pllcfgr |= 1 << (16 + 3 * i + j);
            }
        }
        r.plldivr[i] = d;
        // PLLxFRACR (RM §8.7.13): FRACN 15:3
        r.pllfracr[i] = (p.fracn.unwrap_or(0) as u32) << 3;
    }

    // CFGR (RM §8.7.6): SW 2:0, TIMPRE 15
    let s = &cfg.sys;
    r.cfgr = match s.source {
        SysSource::Hsi => 0,
        SysSource::Csi => 1,
        SysSource::Hse => 2,
        SysSource::Pll1P => 3,
    } | ((s.timpre as u32) << 15);
    // D1CFGR (RM §8.7.7): HPRE 3:0, D1PPRE 6:4, D1CPRE 11:8
    r.d1cfgr = hpre_bits(s.hpre) | (ppre_bits(s.d1ppre) << 4) | (hpre_bits(s.d1cpre) << 8);
    // D2CFGR (RM §8.7.8): D2PPRE1 6:4, D2PPRE2 10:8
    r.d2cfgr = (ppre_bits(s.d2ppre1) << 4) | (ppre_bits(s.d2ppre2) << 8);
    // D3CFGR (RM §8.7.9): D3PPRE 6:4
    r.d3cfgr = ppre_bits(s.d3ppre) << 4;

    use Ker::*;
    let k = &cfg.kernel;
    // D1CCIPR (RM §8.7.18): FMCSEL 1:0, OCTOSPISEL 5:4, SDMMCSEL 16, CKPERSEL 29:28
    let fmc_like = [(Bus, 0), (Pll1Q, 1), (Pll2R, 2), (PerCk, 3)];
    r.d1ccipr = mux(k.fmc, &fmc_like)
        | (mux(k.octospi, &fmc_like) << 4)
        | (mux(k.sdmmc, &[(Pll1Q, 0), (Pll2R, 1)]) << 16)
        | (mux(
            k.per_ck,
            &[(HsiKer, 0), (CsiKer, 1), (Hse, 2), (Disabled, 3)],
        ) << 28);
    // D2CCIP1R (RM §8.7.19): SPI123SEL 14:12, SPI45SEL 18:16, FDCANSEL 29:28
    let spi45_like = [
        (Bus, 0),
        (Pll2Q, 1),
        (Pll3Q, 2),
        (HsiKer, 3),
        (CsiKer, 4),
        (Hse, 5),
    ];
    r.d2ccip1r = (mux(k.spi123, &[(Pll1Q, 0), (Pll2P, 1), (Pll3P, 2), (PerCk, 4)]) << 12)
        | (mux(k.spi45, &spi45_like) << 16)
        | (mux(k.fdcan, &[(Hse, 0), (Pll1Q, 1), (Pll2Q, 2), (Disabled, 3)]) << 28);
    // D2CCIP2R (RM §8.7.20): USART234578SEL 2:0, USART16910SEL 5:3, I2C1235SEL 13:12, USBSEL 21:20
    let usart_like = [
        (Bus, 0),
        (Pll2Q, 1),
        (Pll3Q, 2),
        (HsiKer, 3),
        (CsiKer, 4),
        (Lse, 5),
    ];
    let i2c_like = [(Bus, 0), (Pll3R, 1), (HsiKer, 2), (CsiKer, 3)];
    r.d2ccip2r = mux(k.usart234578, &usart_like)
        | (mux(k.usart16910, &usart_like) << 3)
        | (mux(k.i2c1235, &i2c_like) << 12)
        | (mux(k.usb, &[(Disabled, 0), (Pll1Q, 1), (Pll3Q, 2), (Hsi48, 3)]) << 20);
    // D3CCIPR (RM §8.7.21): LPUART1SEL 2:0, I2C4SEL 9:8, ADCSEL 17:16, SPI6SEL 30:28
    r.d3ccipr = mux(k.lpuart1, &usart_like)
        | (mux(k.i2c4, &i2c_like) << 8)
        | (mux(k.adc, &[(Pll2P, 0), (Pll3R, 1), (PerCk, 2)]) << 16)
        | (mux(k.spi6, &spi45_like) << 28);
    r
}
