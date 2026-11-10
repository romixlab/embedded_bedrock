//! The checker: every finding at once, each with the config key, the exact value, the allowed range and the source.

use exact_units::Exact;
use exact_units::units::Hertz;

use super::config::{Config, CrsSync, HseMode, Ker, Periph, PllSource, SysSource, Vos};
use super::limits::{self, PerRange, Range};
use super::regs::muxes;
use super::tree::{self, Tree};
use crate::finding::{Finding, Severity};
use crate::source::Source;

struct Ctx<'a> {
    cfg: &'a Config,
    out: Vec<Finding>,
}

impl Ctx<'_> {
    fn push(
        &mut self,
        severity: Severity,
        key: impl Into<String>,
        message: String,
        source: Option<Source>,
    ) {
        let f = Finding {
            severity,
            key: key.into(),
            message,
            source,
        };
        if !self.out.contains(&f) {
            self.out.push(f);
        }
    }
    fn error(&mut self, key: impl Into<String>, message: String, source: Source) {
        self.push(Severity::Error, key, message, Some(source));
    }
    fn warn(&mut self, key: impl Into<String>, message: String, source: Source) {
        self.push(Severity::Warning, key, message, Some(source));
    }

    fn in_range(&mut self, key: &str, what: &str, f: Exact<Hertz>, r: &Range) {
        if f < r.min || f > r.max {
            self.error(
                key,
                format!("{what} {f} outside {}..={}", r.min, r.max),
                r.source,
            );
        }
    }

    fn in_per_range(&mut self, key: &str, what: &str, f: Exact<Hertz>, lim: &PerRange) {
        let vos = self.cfg.vos;
        let max = lim.max[vos.idx()];
        if f < lim.min || f > max {
            self.error(
                key,
                format!("{what} {f} outside {}..={max}, the {vos:?} limits", lim.min),
                lim.source,
            );
        }
    }
}

/// Runs every check. Errors and warnings in one list, errors first.
pub fn check(cfg: &Config) -> Vec<Finding> {
    let mut cx = Ctx {
        cfg,
        out: Vec::new(),
    };
    structure(&mut cx);
    match tree::evaluate(cfg) {
        Ok(t) => frequencies(&mut cx, &t),
        Err(e) => cx.push(Severity::Error, "config", e.to_string(), None),
    }
    let mut out = cx.out;
    out.sort_by_key(|f| core::cmp::Reverse(f.severity));
    out
}

/// Checks of the config itself, not of frequencies.
fn structure(cx: &mut Ctx) {
    let cfg = cx.cfg;

    if let Some(h) = cfg.hse {
        let r = match h.mode {
            HseMode::Crystal => limits::HSE_CRYSTAL,
            HseMode::Bypass => limits::HSE_BYPASS,
        };
        cx.in_range("hse.freq", "HSE", h.freq, &r);
        if cfg.vos == Vos::Range2 && h.freq > limits::HSE_RANGE2_MAX {
            cx.error(
                "hse.freq",
                format!("HSE {} above {} in Range 2", h.freq, limits::HSE_RANGE2_MAX),
                limits::CLOCK_PER_RANGE_SRC,
            );
        }
    }
    if cfg.hsi48.is_some() && cfg.vos == Vos::Range2 {
        cx.error(
            "hsi48",
            "HSI48 isn't available in Range 2".into(),
            limits::CLOCK_PER_RANGE_SRC,
        );
    }
    // ES0548 Rev 4 2.2.4: with HSIDIV not ÷1 the device fails to enter Stop on HSE, and clock requests fail to wake it
    if let Some(d) = cfg.hsi16
        && d.ratio() != 1
    {
        cx.warn(
            "hsi16.div",
            format!(
                "HSIDIV ÷{}: Stop fails to enter with SYSCLK on HSE, and peripherals with clock request can't wake the device from Stop (no workaround)",
                d.ratio()
            ),
            Source::Es0548 { erratum: "2.2.4" },
        );
    }

    if let Some(p) = cfg.pll {
        let on = match p.source {
            PllSource::Hse => cfg.hse.is_some(),
            PllSource::Hsi16 => cfg.hsi16.is_some(),
        };
        if !on {
            cx.error(
                "pll.source",
                format!("the PLL takes {:?}, which is not configured", p.source),
                limits::PLLCFGR_SRC,
            );
        }
        if p.p.is_none() && p.q.is_none() && p.r.is_none() {
            cx.warn(
                "pll",
                "PLL with no output in use".into(),
                limits::PLLCFGR_SRC,
            );
        }
    }
    let sys_on = match cfg.sys.source {
        SysSource::Hsisys => cfg.hsi16.is_some(),
        SysSource::Hse => cfg.hse.is_some(),
        SysSource::PllR => cfg.pll.and_then(|p| p.r).is_some(),
    };
    if !sys_on {
        cx.error(
            "sys.source",
            format!("SYSCLK from {:?}, which is off", cfg.sys.source),
            Source::Rm0444 {
                section: "5.2.7",
                page: 171,
            },
        );
    }

    // ES0548 Rev 4 2.2.10: prefetch across the banks fails, no workaround
    let dual = cfg.part.dual_bank(cfg.dual_bank_option);
    if cfg.prefetch && dual && !cfg.allow.prefetch_dual_bank {
        cx.error(
            "flash.prefetch",
            format!(
                "{} runs its flash in dual-bank mode: a prefetch can fail on a branch or call across the banks; prefetch off, or allow.prefetch_dual_bank for code in one bank",
                cfg.part.name
            ),
            limits::PREFETCH_SRC,
        );
    }

    for (key, v, ok) in muxes(&cfg.kernel) {
        if !ok.iter().any(|(c, _)| *c == v) {
            let can: Vec<Ker> = ok.iter().map(|(c, _)| *c).collect();
            cx.error(
                key,
                format!("this mux can't select {v:?} (it can: {can:?})"),
                limits::MUX_SRC,
            );
        }
    }
    if ![1, 2, 4, 8].contains(&cfg.kernel.rng_div) {
        cx.error(
            "kernel.rng_div",
            format!("RNGDIV ÷{} doesn't exist (1, 2, 4, 8)", cfg.kernel.rng_div),
            limits::MUX_SRC,
        );
    }
    if cfg.used.contains(&Periph::Usb)
        && cfg.kernel.usb == Ker::Hsi48
        && cfg.hsi48.is_some_and(|h| h.crs.is_none())
    {
        cx.error(
            "hsi48.crs",
            "USB on HSI48 needs the CRS trimming it (crs = usb-sof)".into(),
            limits::HSI48_SRC,
        );
    }
}

fn mux_of(cfg: &Config, p: Periph) -> Ker {
    let k = &cfg.kernel;
    match p {
        Periph::Adc => k.adc,
        Periph::Rng => k.rng,
        Periph::Tim15 => k.tim15,
        Periph::Tim1 => k.tim1,
        Periph::Lptim2 => k.lptim2,
        Periph::Lptim1 => k.lptim1,
        Periph::I2c2 => k.i2c2,
        Periph::I2c1 => k.i2c1,
        Periph::Lpuart1 => k.lpuart1,
        Periph::Lpuart2 => k.lpuart2,
        Periph::Usart3 => k.usart3,
        Periph::Usart2 => k.usart2,
        Periph::Usart1 => k.usart1,
        Periph::Usb => k.usb,
        Periph::Fdcan => k.fdcan,
        Periph::I2s2 => k.i2s2,
        Periph::I2s1 => k.i2s1,
        Periph::Ucpd => Ker::Hsi16,
    }
}

/// Frequency checks on the evaluated tree.
fn frequencies(cx: &mut Ctx, t: &Tree) {
    let cfg = cx.cfg;

    if let Some(o) = t.pll {
        cx.in_range("pll.m", "PLL input", o.input, &limits::PLL_IN);
        cx.in_per_range("pll.n", "PLL VCO", o.vco, &limits::VCO);
        for (key, name, f, lim) in [
            ("pll.p", "PLLPCLK", o.p, limits::PLL_P),
            ("pll.q", "PLLQCLK", o.q, limits::PLL_Q),
            ("pll.r", "PLLRCLK", o.r, limits::PLL_R),
        ] {
            if let Some(f) = f {
                cx.in_per_range(key, name, f, &lim);
            }
        }
    }

    if let Some(s) = t.sysclk {
        cx.in_per_range("sys.source", "SYSCLK", s, &limits::SYSCLK);
    }
    if let Some(h) = t.hclk {
        cx.in_range("sys.hpre", "HCLK", h, &limits::HCLK_PCLK);
        if flash_wait_states(h, cfg.vos).is_none() {
            cx.error(
                "sys.hpre",
                format!("HCLK {h} has no flash wait-state setting in {:?}", cfg.vos),
                limits::FLASH_WS_SRC,
            );
        }
    }
    if let Some(p) = t.pclk {
        cx.in_range("sys.ppre", "PCLK", p, &limits::HCLK_PCLK);
    }

    for &p in &cfg.used {
        let key = p.key();
        let Some(f) = t.kernel.get(&p).copied().flatten() else {
            let msg = if p == Periph::Ucpd {
                cx.error(
                    key,
                    "UCPD is in use: it runs on HSI16, which is off".into(),
                    limits::UCPD_SRC,
                );
                continue;
            } else {
                format!(
                    "{p:?} is in use but its kernel clock ({:?}) isn't running",
                    mux_of(cfg, p)
                )
            };
            cx.error(key, msg, limits::MUX_SRC);
            continue;
        };
        match p {
            Periph::Adc => {
                if f > limits::ADC_KER_MAX {
                    cx.error(
                        key,
                        format!("ADC kernel clock {f} above {}", limits::ADC_KER_MAX),
                        limits::ADC_KER_SRC,
                    );
                }
                let max = limits::F_ADC.max[cfg.vos.idx()];
                if f > max {
                    cx.warn(
                        key,
                        format!(
                            "ADC kernel clock {f} above fADC {max}: ADC_CCR PRESC must divide it"
                        ),
                        limits::F_ADC.source,
                    );
                }
            }
            Periph::Usb => {
                if f != Exact::new_in::<exact_units::prefix::Mega>(48, 1) {
                    cx.error(
                        key,
                        format!("USB needs exactly 48 MHz, got {f}"),
                        limits::USB_SRC,
                    );
                }
                if let Some(pclk) = t.pclk
                    && pclk < limits::USB_PCLK_MIN
                {
                    cx.error(
                        "sys.ppre",
                        format!(
                            "USB needs PCLK of at least {}, got {pclk}",
                            limits::USB_PCLK_MIN
                        ),
                        limits::USB_SRC,
                    );
                }
            }
            _ => {}
        }
    }

    for e in &cfg.expect {
        let key = format!("expect.{}", &e.periph.key()["kernel.".len()..]);
        let Some(f) = t.kernel.get(&e.periph).copied().flatten() else {
            continue; // reported above when in use
        };
        if let Err(off) = f.within(e.freq, e.tolerance) {
            cx.push(Severity::Error, key, off.to_string(), None);
        }
    }
}

/// The FLASH_ACR wait states for an HCLK in a voltage range (RM0444 Table 12); `None` above the table.
pub fn flash_wait_states(hclk: Exact<Hertz>, vos: Vos) -> Option<u8> {
    limits::FLASH_WS[vos.idx()]
        .iter()
        .position(|top| top.is_some_and(|t| hclk <= t))
        .map(|w| w as u8)
}

/// CRS_CFGR RELOAD and FELIM for a SYNC source: RELOAD = fTARGET / fSYNC − 1 (RM0444 §6.4.3), FELIM =
/// (fTARGET / fSYNC) · 0.14 % / 2 rounded up (§6.4.4, the step its reset value assumes). `None` when fTARGET / fSYNC
/// isn't a whole number or a field doesn't hold the result.
pub fn crs_reload_felim(sync: CrsSync) -> Option<(u16, u8)> {
    use exact_units::Rounding;
    use exact_units::prefix::One;
    let f_sync = match sync {
        CrsSync::UsbSof => limits::USB_SOF,
    };
    let ratio = (limits::HSI48 / f_sync)?.as_ratio();
    let ticks = ratio.to::<One, u32>()?.get();
    let reload = u16::try_from(ticks - 1).ok()?;
    // 0.14 % / 2 = 7 / 10000, rounded up once
    let felim = ratio
        .scaled(Exact::new(7, 10_000))?
        .to_rounded::<One, u32>(Rounding::Ceil)?
        .get();
    Some((reload, u8::try_from(felim).ok()?))
}
