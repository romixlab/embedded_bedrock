//! Every clock of the tree, exactly. Pure evaluation: a clock whose input is missing or whose divider is invalid is
//! `None`, and [check](super::check) says why.

use std::collections::BTreeMap;

use super::config::{Config, Ker, Periph, PllSource, SysSource};
use super::limits;
use crate::ratio::{Hz, Overflow, Ratio};

/// One PLL's clocks.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct PllOut {
    pub ref_ck: Hz,
    /// Wide VCO (VCOSEL = 0) when ref_ck ≥ 2 MHz, else medium (RM §8.5.5 p.327).
    pub wide: bool,
    /// PLLxRGE register value.
    pub rge: u8,
    pub vco: Hz,
    pub p: Option<Hz>,
    pub q: Option<Hz>,
    pub r: Option<Hz>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Tree {
    pub hse: Option<Hz>,
    pub hsi: Option<Hz>,
    pub csi: Option<Hz>,
    pub hsi48: Option<Hz>,
    pub lse: Option<Hz>,
    /// The clock into the DIVMx prescalers.
    pub pll_in: Option<Hz>,
    pub pll: [Option<PllOut>; 3],
    pub sys_ck: Option<Hz>,
    /// After D1CPRE: the Cortex-M7.
    pub cpu: Option<Hz>,
    /// After HPRE: rcc_hclk1..4 and rcc_aclk (AXI) are this one clock.
    pub hclk: Option<Hz>,
    pub pclk1: Option<Hz>,
    pub pclk2: Option<Hz>,
    pub pclk3: Option<Hz>,
    pub pclk4: Option<Hz>,
    /// Timer kernel clocks of the APB1 and APB2 timers (RM Table 53).
    pub tim_apb1: Option<Hz>,
    pub tim_apb2: Option<Hz>,
    pub per_ck: Option<Hz>,
    pub kernel: BTreeMap<Periph, Option<Hz>>,
}

/// PLLxRGE for a reference clock (RM §8.7.11 p.381): 0 for 1..2 MHz, 1 for 2..4, 2 for 4..8, 3 for 8..16.
pub fn rge_for(ref_ck: Hz) -> u8 {
    if ref_ck < Hz::mhz(2) {
        0
    } else if ref_ck < Hz::mhz(4) {
        1
    } else if ref_ck < Hz::mhz(8) {
        2
    } else {
        3
    }
}

fn div(h: Option<Hz>, d: u32) -> Result<Option<Hz>, Overflow> {
    match h {
        Some(h) if d > 0 => Ok(Some(Hz(h.0.div_int(d as u128)?))),
        _ => Ok(None),
    }
}

/// The VCO multiplier N + FRACN/8192.
pub fn multiplier(n: u16, fracn: Option<u16>) -> Result<Ratio, Overflow> {
    Ratio::int(n as u128).checked_add(Ratio::new(fracn.unwrap_or(0) as u128, 8192))
}

pub fn evaluate(cfg: &Config) -> Result<Tree, Overflow> {
    let hse = cfg.hse.map(|h| h.freq);
    let hsi = match cfg.hsi_div {
        Some(d) if matches!(d, 1 | 2 | 4 | 8) => Some(Hz(limits::HSI.0.div_int(d as u128)?)),
        _ => None,
    };
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
        if p.m == 0 || p.n == 0 {
            continue;
        }
        let ref_ck = Hz(input.0.div_int(p.m as u128)?);
        let vco = Hz(ref_ck.0.checked_mul(multiplier(p.n, p.fracn)?)?);
        pll[i] = Some(PllOut {
            ref_ck,
            wide: ref_ck >= Hz::mhz(2),
            rge: rge_for(ref_ck),
            vco,
            p: div(Some(vco), p.p.unwrap_or(0) as u32)?,
            q: div(Some(vco), p.q.unwrap_or(0) as u32)?,
            r: div(Some(vco), p.r.unwrap_or(0) as u32)?,
        });
    }

    let sys_ck = match cfg.sys.source {
        SysSource::Hsi => hsi,
        SysSource::Csi => csi,
        SysSource::Hse => hse,
        SysSource::Pll1P => pll[0].and_then(|p| p.p),
    };
    let s = &cfg.sys;
    let cpu = div(sys_ck, s.d1cpre as u32)?;
    // HPRE divides the D1CPRE output (RM Figure 49, p.332)
    let hclk = div(cpu, s.hpre as u32)?;
    let pclk3 = div(hclk, s.d1ppre as u32)?;
    let pclk1 = div(hclk, s.d2ppre1 as u32)?;
    let pclk2 = div(hclk, s.d2ppre2 as u32)?;
    let pclk4 = div(hclk, s.d3ppre as u32)?;
    let tim = |ppre: u8| -> Result<Option<Hz>, Overflow> {
        // RM Table 53 (p.332-333)
        let direct = if s.timpre {
            matches!(ppre, 1 | 2 | 4)
        } else {
            matches!(ppre, 1 | 2)
        };
        if direct {
            Ok(hclk)
        } else {
            let pclk = div(hclk, ppre as u32)?;
            Ok(match pclk {
                Some(p) => Some(Hz(p.0.mul_int(if s.timpre { 4 } else { 2 })?)),
                None => None,
            })
        }
    };
    let tim_apb1 = tim(s.d2ppre1)?;
    let tim_apb2 = tim(s.d2ppre2)?;

    let src = |k: Ker| -> Option<Hz> {
        let pll_out = |i: usize, f: fn(&PllOut) -> Option<Hz>| pll[i].as_ref().and_then(f);
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
    let pick = |sel: Ker, bus: Option<Hz>| match sel {
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
