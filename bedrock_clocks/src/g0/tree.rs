//! Every clock of the tree, exactly. Pure evaluation: a clock whose input is missing is `None`, and
//! [check](super::check) says why.

use std::collections::BTreeMap;

use exact_units::Exact;
use exact_units::units::Hertz;

use super::config::{Config, Ker, Periph, PllSource, SysSource};
use super::limits;
pub use crate::overflow::Overflow;

/// `x` or [Overflow]: exact_units arithmetic is `None` where the result doesn't fit.
fn fit<T>(x: Option<T>) -> Result<T, Overflow> {
    x.ok_or(Overflow)
}

/// The PLL's clocks.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct PllOut {
    /// After PLLM.
    pub input: Exact<Hertz>,
    pub vco: Exact<Hertz>,
    pub p: Option<Exact<Hertz>>,
    pub q: Option<Exact<Hertz>>,
    pub r: Option<Exact<Hertz>>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Tree {
    pub hse: Option<Exact<Hertz>>,
    pub hsi16: Option<Exact<Hertz>>,
    /// HSI16 ÷ HSIDIV.
    pub hsisys: Option<Exact<Hertz>>,
    pub hsi48: Option<Exact<Hertz>>,
    pub lse: Option<Exact<Hertz>>,
    pub pll: Option<PllOut>,
    pub sysclk: Option<Exact<Hertz>>,
    /// AHB, the core (FCLK) and memory.
    pub hclk: Option<Exact<Hertz>>,
    pub pclk: Option<Exact<Hertz>>,
    /// The timers' clock (TIMPCLK).
    pub timpclk: Option<Exact<Hertz>>,
    pub kernel: BTreeMap<Periph, Option<Exact<Hertz>>>,
}

fn div(h: Option<Exact<Hertz>>, d: u16) -> Result<Option<Exact<Hertz>>, Overflow> {
    h.map(|h| fit(h / i128::from(d))).transpose()
}

pub fn evaluate(cfg: &Config) -> Result<Tree, Overflow> {
    let hse = cfg.hse.map(|h| h.freq);
    let hsi16 = cfg.hsi16.map(|_| limits::HSI16);
    let hsisys = match cfg.hsi16 {
        Some(d) => div(hsi16, d.ratio().into())?,
        None => None,
    };
    let hsi48 = cfg.hsi48.map(|_| limits::HSI48);
    let lse = cfg.lse;

    let pll = match cfg.pll {
        Some(p) => {
            let src = match p.source {
                PllSource::Hsi16 => hsi16,
                PllSource::Hse => hse,
            };
            match src {
                // fVCO = fPLLIN × N / M, each output fVCO / its divider (RM0444 §5.4.4 p.183)
                Some(src) => {
                    let input = fit(src / i128::from(p.m.get()))?;
                    let vco = fit(input * i128::from(p.n.get()))?;
                    let out = |d: Option<u8>| d.map(|d| fit(vco / i128::from(d))).transpose();
                    Some(PllOut {
                        input,
                        vco,
                        p: out(p.p.map(|d| d.get()))?,
                        q: out(p.q.map(|d| d.get()))?,
                        r: out(p.r.map(|d| d.get()))?,
                    })
                }
                None => None,
            }
        }
        None => None,
    };

    let sysclk = match cfg.sys.source {
        SysSource::Hsisys => hsisys,
        SysSource::Hse => hse,
        SysSource::PllR => pll.and_then(|p| p.r),
    };
    let hclk = div(sysclk, cfg.sys.hpre.ratio())?;
    let pclk = div(hclk, cfg.sys.ppre.ratio().into())?;
    let timpclk = if cfg.sys.ppre.ratio() == 1 {
        pclk
    } else {
        pclk.map(|p| fit(p * 2)).transpose()?
    };

    let src = |k: Ker| -> Result<Option<Exact<Hertz>>, Overflow> {
        Ok(match k {
            Ker::Sysclk => sysclk,
            Ker::Pclk => pclk,
            Ker::Timpclk => timpclk,
            Ker::Hsi16 => hsi16,
            Ker::Hsi16Div8 => div(hsi16, 8)?,
            Ker::Hsi48 => hsi48,
            Ker::Hse => hse,
            Ker::Lsi => Some(limits::LSI),
            Ker::Lse => lse,
            Ker::PllP => pll.and_then(|p| p.p),
            Ker::PllQ => pll.and_then(|p| p.q),
            Ker::I2sCkin | Ker::Disabled => None,
        })
    };
    let k = &cfg.kernel;
    let mut kernel = BTreeMap::new();
    for p in Periph::ALL {
        let f = match p {
            Periph::Adc => src(k.adc)?,
            Periph::Rng => div(src(k.rng)?, k.rng_div.into())?,
            Periph::Tim15 => src(k.tim15)?,
            Periph::Tim1 => src(k.tim1)?,
            Periph::Lptim2 => src(k.lptim2)?,
            Periph::Lptim1 => src(k.lptim1)?,
            Periph::I2c2 => src(k.i2c2)?,
            Periph::I2c1 => src(k.i2c1)?,
            Periph::Lpuart1 => src(k.lpuart1)?,
            Periph::Lpuart2 => src(k.lpuart2)?,
            Periph::Usart3 => src(k.usart3)?,
            Periph::Usart2 => src(k.usart2)?,
            Periph::Usart1 => src(k.usart1)?,
            Periph::Usb => src(k.usb)?,
            Periph::Fdcan => src(k.fdcan)?,
            Periph::I2s2 => src(k.i2s2)?,
            Periph::I2s1 => src(k.i2s1)?,
            Periph::Ucpd => hsi16,
        };
        kernel.insert(p, f);
    }

    Ok(Tree {
        hse,
        hsi16,
        hsisys,
        hsi48,
        lse,
        pll,
        sysclk,
        hclk,
        pclk,
        timpclk,
        kernel,
    })
}
