//! Register words for a checked config. The field positions are written out here from RM0444 Rev 5 (§3.7.1
//! FLASH_ACR, §4.4.1 PWR_CR1, §5.4 RCC, §6.7 CRS): stm32-metapac takes one chip per build and this crate's H72x
//! encoding already uses its H725, so `tests/g0_metapac.rs` holds every position to metapac's STM32G0B1CE in a pass
//! of its own (feature `metapac-check-g0`).
//!
//! Every word is the whole register, or the fields `apply()` owns where it says so.

use super::check::{crs_reload_felim, flash_wait_states};
use super::config::{Config, HseMode, Ker, Kernel, PllSource, SysSource, Vos};
use super::tree::Tree;

/// Field positions, `(shift, width)` or a bit.
pub mod field {
    /// PWR_CR1 VOS, bits 10:9: 01 Range 1, 10 Range 2 (RM0444 §4.4.1 p.143).
    pub const PWR_CR1_VOS: (u32, u32) = (9, 2);

    /// FLASH_ACR LATENCY 2:0, PRFTEN 8, ICEN 9 (RM0444 §3.7.1 p.100-101).
    pub const FLASH_ACR_LATENCY: (u32, u32) = (0, 3);
    pub const FLASH_ACR_PRFTEN: u32 = 1 << 8;
    pub const FLASH_ACR_ICEN: u32 = 1 << 9;

    /// RCC_CR (RM0444 §5.4.1 p.178-180).
    pub const CR_HSION: u32 = 1 << 8;
    pub const CR_HSIRDY: u32 = 1 << 10;
    pub const CR_HSIDIV: (u32, u32) = (11, 3);
    pub const CR_HSEON: u32 = 1 << 16;
    pub const CR_HSERDY: u32 = 1 << 17;
    pub const CR_HSEBYP: u32 = 1 << 18;
    pub const CR_HSI48ON: u32 = 1 << 22;
    pub const CR_HSI48RDY: u32 = 1 << 23;
    pub const CR_PLLON: u32 = 1 << 24;
    pub const CR_PLLRDY: u32 = 1 << 25;

    /// RCC_CFGR SW 2:0, SWS 5:3, HPRE 11:8, PPRE 14:12 (RM0444 §5.4.3 p.181-183).
    pub const CFGR_SW: (u32, u32) = (0, 3);
    pub const CFGR_SWS: (u32, u32) = (3, 3);
    pub const CFGR_HPRE: (u32, u32) = (8, 4);
    pub const CFGR_PPRE: (u32, u32) = (12, 3);

    /// RCC_PLLCFGR (RM0444 §5.4.4 p.184-186): each divider field holds the ratio − 1, PLLN the ratio.
    pub const PLLCFGR_PLLSRC: (u32, u32) = (0, 2);
    pub const PLLCFGR_PLLM: (u32, u32) = (4, 3);
    pub const PLLCFGR_PLLN: (u32, u32) = (8, 7);
    pub const PLLCFGR_PLLPEN: u32 = 1 << 16;
    pub const PLLCFGR_PLLP: (u32, u32) = (17, 5);
    pub const PLLCFGR_PLLQEN: u32 = 1 << 24;
    pub const PLLCFGR_PLLQ: (u32, u32) = (25, 3);
    pub const PLLCFGR_PLLREN: u32 = 1 << 28;
    pub const PLLCFGR_PLLR: (u32, u32) = (29, 3);

    /// RCC_CCIPR (RM0444 §5.4.21 p.210-212).
    pub const CCIPR_USART1SEL: (u32, u32) = (0, 2);
    pub const CCIPR_USART2SEL: (u32, u32) = (2, 2);
    pub const CCIPR_USART3SEL: (u32, u32) = (4, 2);
    pub const CCIPR_LPUART2SEL: (u32, u32) = (8, 2);
    pub const CCIPR_LPUART1SEL: (u32, u32) = (10, 2);
    pub const CCIPR_I2C1SEL: (u32, u32) = (12, 2);
    pub const CCIPR_I2C2I2S1SEL: (u32, u32) = (14, 2);
    pub const CCIPR_LPTIM1SEL: (u32, u32) = (18, 2);
    pub const CCIPR_LPTIM2SEL: (u32, u32) = (20, 2);
    pub const CCIPR_TIM1SEL: (u32, u32) = (22, 1);
    pub const CCIPR_TIM15SEL: (u32, u32) = (24, 1);
    pub const CCIPR_RNGSEL: (u32, u32) = (26, 2);
    pub const CCIPR_RNGDIV: (u32, u32) = (28, 2);
    pub const CCIPR_ADCSEL: (u32, u32) = (30, 2);

    /// RCC_CCIPR2 (RM0444 §5.4.22 p.212-213).
    pub const CCIPR2_I2S1SEL: (u32, u32) = (0, 2);
    pub const CCIPR2_I2S2SEL: (u32, u32) = (2, 2);
    pub const CCIPR2_FDCANSEL: (u32, u32) = (8, 2);
    pub const CCIPR2_USBSEL: (u32, u32) = (12, 2);

    /// CRS_CR (RM0444 §6.7.1 p.226): TRIM 14:8 (64 at reset), AUTOTRIMEN 6, CEN 5.
    pub const CRS_CR_TRIM: (u32, u32) = (8, 7);
    pub const CRS_CR_AUTOTRIMEN: u32 = 1 << 6;
    pub const CRS_CR_CEN: u32 = 1 << 5;
    /// CRS_CFGR (RM0444 §6.7.2 p.227-228): RELOAD 15:0, FELIM 23:16, SYNCDIV 26:24, SYNCSRC 29:28.
    pub const CRS_CFGR_RELOAD: (u32, u32) = (0, 16);
    pub const CRS_CFGR_FELIM: (u32, u32) = (16, 8);
    pub const CRS_CFGR_SYNCDIV: (u32, u32) = (24, 3);
    pub const CRS_CFGR_SYNCSRC: (u32, u32) = (28, 2);

    /// `v` placed in field `f`; a value wider than the field is cut to it (the planner's values all fit).
    pub const fn put(f: (u32, u32), v: u32) -> u32 {
        (v & ((1 << f.1) - 1)) << f.0
    }
}

use field::put;

/// The registers `apply()` writes.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct Regs {
    /// PWR_CR1 with only VOS set.
    pub pwr_cr1_vos: u32,
    /// FLASH_ACR LATENCY, PRFTEN and ICEN (the other fields stay as they are: EMPTY and DBG_SWEN aren't the plan's).
    pub flash_acr: u32,
    /// RCC_CR bits to set for the oscillators (HSION and HSIDIV, HSEON, HSEBYP, HSI48ON), without PLLON.
    pub rcc_cr_osc: u32,
    /// RCC_CR PLLON when the PLL is used.
    pub rcc_cr_pll_on: u32,
    /// RCC_CR bits of the oscillators that stay off at the end (HSION when HSI16 isn't used).
    pub rcc_cr_off: u32,
    pub pllcfgr: u32,
    /// SW, HPRE and PPRE.
    pub cfgr: u32,
    pub ccipr: u32,
    pub ccipr2: u32,
    /// CRS_CFGR, written before CRS_CR turns the counter on (it is write-protected after); 0 without the CRS.
    pub crs_cfgr: u32,
    /// CRS_CR AUTOTRIMEN and CEN with TRIM at its reset value 64; 0 without the CRS.
    pub crs_cr: u32,
}

/// PWR_CR1 VOS (RM0444 §4.4.1 p.143).
pub const fn vos_bits(v: Vos) -> u32 {
    put(
        field::PWR_CR1_VOS,
        match v {
            Vos::Range1 => 0b01,
            Vos::Range2 => 0b10,
        },
    )
}

/// A kernel mux: its config key, the selection, and each choice with its field value.
pub type Mux = (&'static str, Ker, &'static [(Ker, u8)]);

/// The kernel muxes of RCC_CCIPR / CCIPR2 (RM0444 §5.4.21-5.4.22). One table for the checker (a selection not
/// listed is refused) and the encoder; the order is [mux_fields]'.
pub fn muxes(k: &Kernel) -> [Mux; 17] {
    use Ker::*;
    const UART: &[(Ker, u8)] = &[(Pclk, 0), (Sysclk, 1), (Hsi16, 2), (Lse, 3)];
    const I2C: &[(Ker, u8)] = &[(Pclk, 0), (Sysclk, 1), (Hsi16, 2)];
    const LPTIM: &[(Ker, u8)] = &[(Pclk, 0), (Lsi, 1), (Hsi16, 2), (Lse, 3)];
    const TIM: &[(Ker, u8)] = &[(Timpclk, 0), (PllQ, 1)];
    const I2S: &[(Ker, u8)] = &[(Sysclk, 0), (PllP, 1), (Hsi16, 2), (I2sCkin, 3)];
    [
        ("kernel.usart1", k.usart1, UART),
        ("kernel.usart2", k.usart2, UART),
        ("kernel.usart3", k.usart3, UART),
        ("kernel.lpuart2", k.lpuart2, UART),
        ("kernel.lpuart1", k.lpuart1, UART),
        ("kernel.i2c1", k.i2c1, I2C),
        ("kernel.i2c2", k.i2c2, I2C),
        ("kernel.lptim1", k.lptim1, LPTIM),
        ("kernel.lptim2", k.lptim2, LPTIM),
        ("kernel.tim1", k.tim1, TIM),
        ("kernel.tim15", k.tim15, TIM),
        (
            "kernel.rng",
            k.rng,
            &[(Disabled, 0), (Hsi16Div8, 1), (Sysclk, 2), (PllQ, 3)],
        ),
        ("kernel.adc", k.adc, &[(Sysclk, 0), (PllP, 1), (Hsi16, 2)]),
        ("kernel.i2s1", k.i2s1, I2S),
        ("kernel.i2s2", k.i2s2, I2S),
        ("kernel.fdcan", k.fdcan, &[(Pclk, 0), (PllQ, 1), (Hse, 2)]),
        ("kernel.usb", k.usb, &[(Hsi48, 0), (PllQ, 1), (Hse, 2)]),
    ]
}

/// Where each mux of [muxes] lives: CCIPR (false) or CCIPR2 (true), and its field.
pub const fn mux_fields() -> [(bool, (u32, u32)); 17] {
    use field::*;
    [
        (false, CCIPR_USART1SEL),
        (false, CCIPR_USART2SEL),
        (false, CCIPR_USART3SEL),
        (false, CCIPR_LPUART2SEL),
        (false, CCIPR_LPUART1SEL),
        (false, CCIPR_I2C1SEL),
        (false, CCIPR_I2C2I2S1SEL),
        (false, CCIPR_LPTIM1SEL),
        (false, CCIPR_LPTIM2SEL),
        (false, CCIPR_TIM1SEL),
        (false, CCIPR_TIM15SEL),
        (false, CCIPR_RNGSEL),
        (false, CCIPR_ADCSEL),
        (true, CCIPR2_I2S1SEL),
        (true, CCIPR2_I2S2SEL),
        (true, CCIPR2_FDCANSEL),
        (true, CCIPR2_USBSEL),
    ]
}

/// Encodes a config the checker passed without errors.
pub fn encode(cfg: &Config, t: &Tree) -> Regs {
    let ws = flash_wait_states(t.hclk.expect("checked"), cfg.vos).expect("checked");
    let mut flash_acr = put(field::FLASH_ACR_LATENCY, ws.into());
    if cfg.prefetch {
        flash_acr |= field::FLASH_ACR_PRFTEN;
    }
    if cfg.icache {
        flash_acr |= field::FLASH_ACR_ICEN;
    }

    let (mut osc, mut off) = (0, 0);
    match cfg.hsi16 {
        Some(d) => osc |= field::CR_HSION | put(field::CR_HSIDIV, d.bits().into()),
        None => off |= field::CR_HSION,
    }
    if let Some(h) = cfg.hse {
        osc |= field::CR_HSEON;
        if h.mode == HseMode::Bypass {
            osc |= field::CR_HSEBYP;
        }
    }
    if cfg.hsi48.is_some() {
        osc |= field::CR_HSI48ON;
    }

    let (mut pllcfgr, mut pll_on) = (0, 0);
    if let Some(p) = cfg.pll {
        pll_on = field::CR_PLLON;
        pllcfgr |= put(
            field::PLLCFGR_PLLSRC,
            match p.source {
                PllSource::Hsi16 => 0b10,
                PllSource::Hse => 0b11,
            },
        );
        pllcfgr |= put(field::PLLCFGR_PLLM, u32::from(p.m.get()) - 1);
        pllcfgr |= put(field::PLLCFGR_PLLN, p.n.get().into());
        // an unused output has its enable bit at 0 and its divider at ÷2 (field 1): field 0 is a reserved encoding,
        // though the register resets to it (RM0444 §5.4.4)
        let out = |d: Option<u8>| u32::from(d.unwrap_or(2) - 1);
        pllcfgr |= put(field::PLLCFGR_PLLP, out(p.p.map(|d| d.get())));
        pllcfgr |= put(field::PLLCFGR_PLLQ, out(p.q.map(|d| d.get())));
        pllcfgr |= put(field::PLLCFGR_PLLR, out(p.r.map(|d| d.get())));
        if p.p.is_some() {
            pllcfgr |= field::PLLCFGR_PLLPEN;
        }
        if p.q.is_some() {
            pllcfgr |= field::PLLCFGR_PLLQEN;
        }
        if p.r.is_some() {
            pllcfgr |= field::PLLCFGR_PLLREN;
        }
    }

    let s = &cfg.sys;
    let sw = match s.source {
        SysSource::Hsisys => 0,
        SysSource::Hse => 1,
        SysSource::PllR => 2,
    };
    let cfgr = put(field::CFGR_SW, sw)
        | put(field::CFGR_HPRE, s.hpre.bits().into())
        | put(field::CFGR_PPRE, s.ppre.bits().into());

    let (mut ccipr, mut ccipr2) = (0, 0);
    for ((_, sel, ok), (two, f)) in muxes(&cfg.kernel).iter().zip(mux_fields()) {
        let v = ok
            .iter()
            .find(|(c, _)| c == sel)
            .map(|(_, v)| *v)
            .expect("checked mux choice");
        if two {
            ccipr2 |= put(f, v.into());
        } else {
            ccipr |= put(f, v.into());
        }
    }
    ccipr |= put(field::CCIPR_RNGDIV, cfg.kernel.rng_div.trailing_zeros());

    let (mut crs_cfgr, mut crs_cr) = (0, 0);
    if let Some(sync) = cfg.hsi48.and_then(|h| h.crs) {
        let (reload, felim) = crs_reload_felim(sync).expect("1 kHz SOF into 48 MHz");
        // SYNCSRC 10: USB SOF (RM0444 Table 38), not divided, rising edge
        crs_cfgr = put(field::CRS_CFGR_RELOAD, reload.into())
            | put(field::CRS_CFGR_FELIM, felim.into())
            | put(field::CRS_CFGR_SYNCSRC, 0b10);
        crs_cr = put(field::CRS_CR_TRIM, 64) | field::CRS_CR_AUTOTRIMEN | field::CRS_CR_CEN;
    }

    Regs {
        pwr_cr1_vos: vos_bits(cfg.vos),
        flash_acr,
        rcc_cr_osc: osc,
        rcc_cr_pll_on: pll_on,
        rcc_cr_off: off,
        pllcfgr,
        cfgr,
        ccipr,
        ccipr2,
        crs_cfgr,
        crs_cr,
    }
}
