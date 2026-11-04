//! Every clock of the tree, exactly. Pure evaluation: a clock whose input is missing or whose divider is invalid is
//! `None`, and [check](super::check) says why.

use std::collections::BTreeMap;

use core::fmt;

use exact_units::Exact;
use exact_units::prefix::Mega;
use exact_units::units::Hertz;
use stm32_metapac::rcc::vals::Pllrge;

use super::config::{Config, Divpqr, Ker, Periph, PllSource, Ppre, SysSource};
use super::limits;

/// An exact step of the tree went past what a `Fraction` holds (u128 numerator and denominator).
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct Overflow;

impl fmt::Display for Overflow {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("exact arithmetic overflowed u128")
    }
}

impl std::error::Error for Overflow {}

/// `x` or [Overflow]: exact_units arithmetic is `None` where the result doesn't fit.
fn fit<T>(x: Option<T>) -> Result<T, Overflow> {
    x.ok_or(Overflow)
}

/// One PLL's clocks.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct PllOut {
    pub ref_ck: Exact<Hertz>,
    /// Wide VCO (VCOSEL = 0) when ref_ck ≥ 2 MHz, else medium (RM §8.5.5 p.327).
    pub wide: bool,
    pub rge: Pllrge,
    pub vco: Exact<Hertz>,
    pub p: Option<Exact<Hertz>>,
    pub q: Option<Exact<Hertz>>,
    pub r: Option<Exact<Hertz>>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Tree {
    pub hse: Option<Exact<Hertz>>,
    pub hsi: Option<Exact<Hertz>>,
    pub csi: Option<Exact<Hertz>>,
    pub hsi48: Option<Exact<Hertz>>,
    pub lse: Option<Exact<Hertz>>,
    /// The clock into the DIVMx prescalers.
    pub pll_in: Option<Exact<Hertz>>,
    pub pll: [Option<PllOut>; 3],
    pub sys_ck: Option<Exact<Hertz>>,
    /// After D1CPRE: the Cortex-M7.
    pub cpu: Option<Exact<Hertz>>,
    /// After HPRE: rcc_hclk1..4 and rcc_aclk (AXI) are this one clock.
    pub hclk: Option<Exact<Hertz>>,
    pub pclk1: Option<Exact<Hertz>>,
    pub pclk2: Option<Exact<Hertz>>,
    pub pclk3: Option<Exact<Hertz>>,
    pub pclk4: Option<Exact<Hertz>>,
    /// Timer kernel clocks of the APB1 and APB2 timers (RM Table 53).
    pub tim_apb1: Option<Exact<Hertz>>,
    pub tim_apb2: Option<Exact<Hertz>>,
    pub per_ck: Option<Exact<Hertz>>,
    pub kernel: BTreeMap<Periph, Option<Exact<Hertz>>>,
}

/// PLLxRGE for a reference clock (RM §8.7.11 p.381): 1..2 MHz, 2..4, 4..8, 8..16.
pub fn rge_for(ref_ck: Exact<Hertz>) -> Pllrge {
    let mhz = |n| Exact::new_in::<Mega>(n, 1);
    if ref_ck < mhz(2) {
        Pllrge::RANGE1
    } else if ref_ck < mhz(4) {
        Pllrge::RANGE2
    } else if ref_ck < mhz(8) {
        Pllrge::RANGE4
    } else {
        Pllrge::RANGE8
    }
}

fn div(h: Option<Exact<Hertz>>, d: u16) -> Result<Option<Exact<Hertz>>, Overflow> {
    h.map(|h| fit(h / i128::from(d))).transpose()
}

pub fn evaluate(cfg: &Config) -> Result<Tree, Overflow> {
    let hse = cfg.hse.map(|h| h.freq);
    let hsi = cfg
        .hsi_div
        .map(|d| fit(limits::HSI / i128::from(d.ratio())))
        .transpose()?;
    let csi = cfg.csi.then_some(limits::CSI);
    let hsi48 = cfg.hsi48.then_some(limits::HSI48);
    let lse = cfg.lse;
    let pll_in = match cfg.pll_source {
        PllSource::Hse => hse,
        PllSource::Hsi => hsi,
        PllSource::Csi => csi,
    };

    let mut pll = [None; 3];
    for (i, p) in cfg.pll.iter().enumerate() {
        let (Some(p), Some(input)) = (p, pll_in) else {
            continue;
        };
        // ref_ck = input / M, VCO = ref_ck · (N + FRACN/8192), each output VCO / its divider
        let ref_ck = fit(input / i128::from(p.m.get()))?;
        let vco = fit(ref_ck.scaled(fit(p.multiplier())?))?;
        let out = |d: Option<Divpqr>| d.map(|d| fit(vco / i128::from(d.get()))).transpose();
        pll[i] = Some(PllOut {
            ref_ck,
            wide: ref_ck >= Exact::new_in::<Mega>(2, 1),
            rge: rge_for(ref_ck),
            vco,
            p: out(p.p)?,
            q: out(p.q)?,
            r: out(p.r)?,
        });
    }

    let sys_ck = match cfg.sys.source {
        SysSource::Hsi => hsi,
        SysSource::Csi => csi,
        SysSource::Hse => hse,
        SysSource::Pll1P => pll[0].and_then(|p| p.p),
    };
    let s = &cfg.sys;
    let cpu = div(sys_ck, s.d1cpre.ratio())?;
    // HPRE divides the D1CPRE output (RM Figure 49, p.332)
    let hclk = div(cpu, s.hpre.ratio())?;
    let pclk = |p: Ppre| div(hclk, p.ratio().into());
    let pclk3 = pclk(s.d1ppre)?;
    let pclk1 = pclk(s.d2ppre1)?;
    let pclk2 = pclk(s.d2ppre2)?;
    let pclk4 = pclk(s.d3ppre)?;
    let tim = |ppre: Ppre| -> Result<Option<Exact<Hertz>>, Overflow> {
        // RM Table 53 (p.332-333)
        let direct = if s.timpre {
            matches!(ppre, Ppre::Div1 | Ppre::Div2 | Ppre::Div4)
        } else {
            matches!(ppre, Ppre::Div1 | Ppre::Div2)
        };
        if direct {
            Ok(hclk)
        } else {
            let times = if s.timpre { 4 } else { 2 };
            pclk(ppre)?.map(|p| fit(p * times)).transpose()
        }
    };
    let tim_apb1 = tim(s.d2ppre1)?;
    let tim_apb2 = tim(s.d2ppre2)?;

    let src = |k: Ker| -> Option<Exact<Hertz>> {
        let pll_out =
            |i: usize, f: fn(&PllOut) -> Option<Exact<Hertz>>| pll[i].as_ref().and_then(f);
        match k {
            Ker::Pll1Q => pll_out(0, |p| p.q),
            Ker::Pll2P => pll_out(1, |p| p.p),
            Ker::Pll2Q => pll_out(1, |p| p.q),
            Ker::Pll2R => pll_out(1, |p| p.r),
            Ker::Pll3P => pll_out(2, |p| p.p),
            Ker::Pll3Q => pll_out(2, |p| p.q),
            Ker::Pll3R => pll_out(2, |p| p.r),
            Ker::Hse => hse,
            // hsi_ker_ck / csi_ker_ck are HSI / CSI with HSIKERON / CSIKERON (not modelled: on when the clock is)
            Ker::HsiKer => hsi,
            Ker::CsiKer => csi,
            Ker::Hsi48 => hsi48,
            Ker::Lse => lse,
            Ker::PerCk | Ker::Bus | Ker::Disabled => None,
        }
    };
    let per_ck = match cfg.kernel.per_ck {
        Ker::HsiKer => hsi,
        Ker::CsiKer => csi,
        Ker::Hse => hse,
        _ => None,
    };
    let k = &cfg.kernel;
    let pick = |sel: Ker, bus: Option<Exact<Hertz>>| match sel {
        Ker::Bus => bus,
        Ker::PerCk => per_ck,
        other => src(other),
    };
    let mut kernel = BTreeMap::new();
    for p in Periph::ALL {
        let f = match p {
            Periph::Sdmmc => pick(k.sdmmc, None),
            Periph::Octospi => pick(k.octospi, hclk),
            Periph::Fmc => pick(k.fmc, hclk),
            Periph::Fdcan => pick(k.fdcan, None),
            Periph::Spi45 => pick(k.spi45, pclk2),
            Periph::Spi123 => pick(k.spi123, None),
            Periph::UsbOtgHs => pick(k.usb, None),
            Periph::I2c1235 => pick(k.i2c1235, pclk1),
            Periph::Usart16910 => pick(k.usart16910, pclk2),
            Periph::Usart234578 => pick(k.usart234578, pclk1),
            Periph::Spi6 => pick(k.spi6, pclk4),
            Periph::Adc => pick(k.adc, None),
            Periph::I2c4 => pick(k.i2c4, pclk4),
            Periph::Lpuart1 => pick(k.lpuart1, pclk4),
        };
        kernel.insert(p, f);
    }

    Ok(Tree {
        hse,
        hsi,
        csi,
        hsi48,
        lse,
        pll_in,
        pll,
        sys_ck,
        cpu,
        hclk,
        pclk1,
        pclk2,
        pclk3,
        pclk4,
        tim_apb1,
        tim_apb2,
        per_ck,
        kernel,
    })
}
