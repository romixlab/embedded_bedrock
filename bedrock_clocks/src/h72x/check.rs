//! The checker: every finding at once, each with the config key, the exact value, the allowed range and the source.

use core::fmt;

use exact_units::prefix::{Mega, Milli, Rational};
use exact_units::units::{Hertz, Ratio};
use exact_units::{Exact, Rounding};

use super::config::{
    Config, Fracn, Ker, Line, Package, Periph, Pll, PllSource, SteerPpm, Supply, SysSource,
    TempRange, Vos,
};
use super::limits::{self, PerVos, Range};
use super::regs::muxes;
use super::tree::{self, Tree};
use crate::source::Source;

#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Severity {
    Warning,
    Error,
}

/// One problem with a config.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Finding {
    pub severity: Severity,
    /// The config key it is about: `pll1.n`, `sys.hpre`, `kernel.fdcan`.
    pub key: String,
    pub message: String,
    pub source: Option<Source>,
}

impl fmt::Display for Finding {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let sev = match self.severity {
            Severity::Error => "error",
            Severity::Warning => "warning",
        };
        write!(f, "{sev}: {}: {}", self.key, self.message)?;
        if let Some(s) = &self.source {
            write!(f, " [{s}]")?;
        }
        Ok(())
    }
}

struct Ctx<'a> {
    cfg: &'a Config,
    out: Vec<Finding>,
    /// " at the -100 ppm steer end (PLL1 FRACN 2588)" while checking a steer end.
    at: String,
}

impl Ctx<'_> {
    fn push(
        &mut self,
        severity: Severity,
        key: impl Into<String>,
        message: String,
        source: Option<Source>,
    ) {
        let message = format!("{message}{}", self.at);
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

    fn at_most(&mut self, key: &str, what: &str, f: Exact<Hertz>, lim: &PerVos, vos: Vos) {
        let max = lim.max[vos.idx()];
        if f > max {
            self.error(
                key,
                format!("{what} {f} above {max}, the {vos:?} maximum"),
                lim.source,
            );
        }
    }
}

/// FRACN at the ends of a steer range: the multiplier N + FRACN/8192 scaled by (1 + ppm), in FRACN steps, rounded
/// inwards (up at the bottom end, down at the top) so the end stays inside the range. `None` when the PLL has no
/// FRACN or no steer range.
pub fn fracn_span(p: &Pll) -> Option<(i32, i32)> {
    // integer mode has nothing to steer
    p.fracn?;
    let (steer, mult) = (p.steer?, p.multiplier()?);
    let n = Exact::from_fixed(p.n)?;
    let end = |ppm, r| {
        let scale = (Exact::<Ratio>::from_fixed(ppm)? + Exact::int(1))?;
        ((mult * scale.get())? - n)?.to_rounded::<Rational<1, 8192>, i32>(r)
    };
    let SteerPpm { min, max } = steer;
    Some((
        end(min, Rounding::Ceil)?.get(),
        end(max, Rounding::Floor)?.get(),
    ))
}

/// Runs every check. Errors and warnings in one list, errors first.
pub fn check(cfg: &Config) -> Vec<Finding> {
    let mut cx = Ctx {
        cfg,
        out: Vec::new(),
        at: String::new(),
    };
    structure(&mut cx);
    match tree::evaluate(cfg) {
        Ok(t) => frequencies(&mut cx, &t),
        Err(e) => cx.push(Severity::Error, "config", e.to_string(), None),
    }

    // the same frequency checks at both ends of every steer range
    for (i, p) in cfg.pll.iter().enumerate() {
        let Some(p) = p else { continue };
        let (Some((lo, hi)), Some(s)) = (fracn_span(p), p.steer) else {
            continue;
        };
        let key = format!("pll{}.steer", i + 1);
        for (ppm, fr) in [(s.min.get(), lo), (s.max.get(), hi)] {
            let Some(f) = u16::try_from(fr).ok().and_then(|f| Fracn::try_new(f).ok()) else {
                cx.at.clear();
                cx.error(
                    key.as_str(),
                    format!(
                        "the {ppm:+} ppm end needs FRACN {fr}, outside 1..={}: steering changes FRACN only, and FRACN 0 is integer mode (pick another N or a smaller range)",
                        limits::FRACN_MAX
                    ),
                    limits::FRACN_SRC,
                );
                continue;
            };
            let mut c2 = cfg.clone();
            if let Some(pp) = c2.pll[i].as_mut() {
                pp.fracn = Some(f);
            }
            cx.at = format!(
                " (at the {ppm:+} ppm steer end of PLL{}, FRACN {fr})",
                i + 1
            );
            if let Ok(t) = tree::evaluate(&c2) {
                frequencies(&mut cx, &t);
            }
        }
        cx.at.clear();
    }
    let mut out = cx.out;
    out.sort_by_key(|f| core::cmp::Reverse(f.severity));
    out
}

/// Checks of the config itself, not of frequencies.
fn structure(cx: &mut Ctx) {
    let cfg = cx.cfg;

    // supply against the package: the H725 VFQFPN68 has no LDO (DS13311 Table 2, p.20)
    let ldo_used = matches!(
        cfg.supply,
        Supply::Default | Supply::Ldo | Supply::SmpsLdo(_) | Supply::SmpsExternalLdo(_)
    );
    if cfg.part.line == Line::H725 && cfg.part.package == Package::Vfqfpn && ldo_used {
        cx.error(
            "supply",
            format!(
                "{} has no LDO: use direct-smps or a bypass configuration",
                cfg.part.name
            ),
            Source::Ds13311 { table: 2, page: 20 },
        );
    }
    if cfg.part.line != Line::H725 {
        cx.warn(
            "part",
            format!(
                "{:?}: supply and package rules come from the H725 datasheet only; check this part's datasheet",
                cfg.part.line
            ),
            Source::Ds13311 { table: 2, page: 20 },
        );
    }

    // VOS0: VDD minimum per supply and the 105 °C junction limit (DS13311 Table 14)
    if cfg.vos == Vos::Vos0 {
        let kind = match cfg.supply {
            Supply::DirectSmps => "smps",
            Supply::Ldo | Supply::Default => "ldo",
            Supply::SmpsLdo(_) | Supply::SmpsExternalLdo(_) => "smps-ldo",
            Supply::Bypass | Supply::SmpsExternalBypass(_) => "bypass",
        };
        let min = limits::VOS0_VDD_MIN
            .iter()
            .find(|(k, _)| *k == kind)
            .map(|(_, v)| *v)
            .unwrap();
        if cfg.vdd < min {
            cx.error(
                "vdd",
                format!(
                    "VOS0 with a {kind} supply needs VDD ≥ {min}, the board has {}",
                    cfg.vdd
                ),
                limits::VOS0_TJ_SRC,
            );
        }
        if cfg.part.temp == TempRange::Extended && !cfg.allow.vos0_on_extended_temp {
            cx.warn(
                "vos",
                "VOS0 limits the junction to 105 °C on this extended-temperature part (allow.vos0_on_extended_temp)"
                    .into(),
                limits::VOS0_TJ_SRC,
            );
        }
    }
    if cfg.cpu_boost && cfg.vos != Vos::Vos0 {
        cx.error(
            "cpu_boost",
            "CPU frequency boost needs VOS0".into(),
            Source::Ds13311 {
                table: 13,
                page: 118,
            },
        );
    }
    if cfg.cpu_boost && !cfg.allow.cpu_boost_tcm_ecc_off {
        cx.error(
            "cpu_boost",
            "CPUFREQ_BOOST turns the ITCM / DTCM ECC off: refused unless allow.cpu_boost_tcm_ecc_off".into(),
            Source::Rm0468 {
                section: "4.9.25",
                page: 218,
            },
        );
    }
    if cfg.vos == Vos::Vos0 && !cfg.allow.vos0_lifetime {
        cx.warn(
            "vos",
            "VOS0 wears the part out: about 2 years at TJ 105 °C and 3.5 years at 95 °C with the core busy all the time, \
             10 years only at 20 % duty; VOS1 (CPU ≤ 400 MHz) is over 10 years at 105 °C (allow.vos0_lifetime)"
                .into(),
            Source::An5337 {
                section: "7",
                page: 10,
            },
        );
    }
    // ES0491 Rev 9 2.16.1: WWDG not functional below 2.7 V at VOS0/VOS1, no workaround
    if cfg.vdd < Exact::new_in::<Milli>(2700, 1) && cfg.vos <= Vos::Vos1 {
        cx.warn(
            "vdd",
            format!("the WWDG doesn't work at {} with {:?}", cfg.vdd, cfg.vos),
            Source::Es0491 { erratum: "2.16.1" },
        );
    }

    // oscillators
    if let Some(h) = cfg.hse {
        cx.in_range("hse.freq", "HSE", h.freq, &limits::HSE);
    }
    let src_on = match cfg.pll_source {
        PllSource::Hse => cfg.hse.is_some(),
        PllSource::Hsi => cfg.hsi_div.is_some(),
        PllSource::Csi => cfg.csi,
    };
    if cfg.pll.iter().any(|p| p.is_some()) && !src_on {
        cx.error(
            "pll.source",
            format!(
                "the PLLs take {:?}, which is not configured",
                cfg.pll_source
            ),
            limits::PLLSRC_SRC,
        );
    }

    // PLL dividers
    for (i, p) in cfg.pll.iter().enumerate() {
        let Some(p) = p else { continue };
        let k = |f: &str| format!("pll{}.{f}", i + 1);
        if i == 0
            && let Some(pd) = p.p.map(|d| d.get())
            && pd != 1
            && pd % 2 == 1
        {
            cx.error(
                k("p"),
                format!("PLL1 DIVP ÷{pd}: only ÷1 or even ratios"),
                limits::PLL1_DIVP_EVEN_SRC,
            );
        }
        if p.p.is_none() && p.q.is_none() && p.r.is_none() {
            cx.warn(
                k("p"),
                "PLL with no output in use".into(),
                limits::FRACN_SRC,
            );
        }
        if let Some(s) = p.steer {
            if p.fracn.is_none() {
                cx.error(
                    k("steer"),
                    "a steer range needs fractional mode: set fracn".into(),
                    limits::FRACN_SRC,
                );
            }
            if s.min > s.max {
                cx.error(
                    k("steer"),
                    format!("steer range {}..{} is empty", s.min, s.max),
                    limits::FRACN_SRC,
                );
            }
        }
    }

    let s = &cfg.sys;
    if s.source == SysSource::Pll1P && cfg.pll[0].and_then(|p| p.p).is_none() {
        cx.error(
            "sys.source",
            "sys_ck from PLL1 P, but PLL1 P is off".into(),
            limits::FRACN_SRC,
        );
    }

    // kernel mux choices (RM §8.7.18-8.7.21)
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
}

fn pll_ratio_of(cfg: &Config, k: Ker) -> Option<u8> {
    let p = |i: usize| cfg.pll[i].as_ref();
    match k {
        Ker::Pll1Q => p(0)?.q,
        Ker::Pll2P => p(1)?.p,
        Ker::Pll2Q => p(1)?.q,
        Ker::Pll2R => p(1)?.r,
        Ker::Pll3P => p(2)?.p,
        Ker::Pll3Q => p(2)?.q,
        Ker::Pll3R => p(2)?.r,
        _ => None,
    }
    .map(|d| d.get())
}

fn mux_of(cfg: &Config, p: Periph) -> Ker {
    let k = &cfg.kernel;
    match p {
        Periph::Sdmmc => k.sdmmc,
        Periph::Octospi => k.octospi,
        Periph::Fmc => k.fmc,
        Periph::Fdcan => k.fdcan,
        Periph::Spi45 => k.spi45,
        Periph::Spi123 => k.spi123,
        Periph::UsbOtgHs => k.usb,
        Periph::I2c1235 => k.i2c1235,
        Periph::Usart16910 => k.usart16910,
        Periph::Usart234578 => k.usart234578,
        Periph::Spi6 => k.spi6,
        Periph::Adc => k.adc,
        Periph::I2c4 => k.i2c4,
        Periph::Lpuart1 => k.lpuart1,
    }
}

/// Frequency checks on one evaluated tree (the nominal one, or a steer end).
fn frequencies(cx: &mut Ctx, t: &Tree) {
    let cfg = cx.cfg;
    let vos = cfg.vos;

    for (i, p) in cfg.pll.iter().enumerate() {
        let (Some(p), Some(o)) = (p, t.pll[i]) else {
            continue;
        };
        let k = |f: &str| format!("pll{}.{f}", i + 1);
        cx.in_range(
            &k("m"),
            &format!("PLL{} ref_ck", i + 1),
            o.ref_ck,
            &limits::PLL_REF,
        );
        let vco = if o.wide {
            limits::VCO_WIDE
        } else {
            limits::VCO_MEDIUM
        };
        cx.in_range(
            &k("n"),
            &format!(
                "PLL{} VCO ({})",
                i + 1,
                if o.wide { "wide" } else { "medium" }
            ),
            o.vco,
            &vco,
        );
        if !o.wide && p.fracn.is_some() {
            cx.error(
                k("fracn"),
                format!(
                    "fractional mode with the medium VCO (ref_ck {} < 2 MHz)",
                    o.ref_ck
                ),
                limits::FRACN_MEDIUM_SRC,
            );
        }
        let (lim, min) = if o.wide {
            (limits::PLL_OUT_WIDE, limits::PLL_OUT_WIDE_MIN)
        } else {
            (limits::PLL_OUT_MEDIUM, limits::PLL_OUT_MEDIUM_MIN)
        };
        for (name, out) in [("p", o.p), ("q", o.q), ("r", o.r)] {
            let Some(f) = out else { continue };
            cx.at_most(
                &k(name),
                &format!("PLL{} {}", i + 1, name.to_uppercase()),
                f,
                &lim,
                vos,
            );
            if f < min {
                cx.error(
                    k(name),
                    format!("PLL{} {} {f} below {min}", i + 1, name.to_uppercase()),
                    lim.source,
                );
            }
        }
    }

    // buses
    match t.sys_ck {
        None => cx.error(
            "sys.source",
            "sys_ck has no running source".into(),
            limits::FRACN_SRC,
        ),
        Some(_) => {
            if let Some(cpu) = t.cpu {
                if cfg.cpu_boost && vos == Vos::Vos0 {
                    if cpu > limits::CPU_BOOST {
                        cx.error(
                            "sys.d1cpre",
                            format!("CPU {cpu} above {} with boost", limits::CPU_BOOST),
                            limits::CPU.source,
                        );
                    }
                } else {
                    cx.at_most("sys.d1cpre", "CPU", cpu, &limits::CPU, vos);
                }
            }
            if let Some(h) = t.hclk {
                cx.at_most("sys.hpre", "AXI/AHB clock", h, &limits::AXI_AHB, vos);
            }
            for (key, name, f) in [
                ("sys.d1ppre", "APB3", t.pclk3),
                ("sys.d2ppre1", "APB1", t.pclk1),
                ("sys.d2ppre2", "APB2", t.pclk2),
                ("sys.d3ppre", "APB4", t.pclk4),
            ] {
                if let Some(f) = f {
                    cx.at_most(key, name, f, &limits::APB, vos);
                }
            }
            for (key, name, f) in [
                ("sys.timpre", "APB1 timer clock", t.tim_apb1),
                ("sys.timpre", "APB2 timer clock", t.tim_apb2),
            ] {
                if let Some(f) = f {
                    cx.at_most(key, name, f, &limits::TIMERS, vos);
                }
            }
        }
    }

    // kernel clocks of the peripherals in use
    for &p in &cfg.used {
        let key = p.key();
        let f = t.kernel.get(&p).copied().flatten();
        let Some(f) = f else {
            cx.error(
                key,
                format!(
                    "{p:?} is in use but its kernel clock ({:?}) isn't running",
                    mux_of(cfg, p)
                ),
                limits::MUX_SRC,
            );
            continue;
        };
        cx.at_most(
            key,
            &format!("{p:?} kernel clock"),
            f,
            &limits::kernel_max(p),
            vos,
        );
        if matches!(p, Periph::Sdmmc | Periph::Adc)
            && let Some(r) = pll_ratio_of(cfg, mux_of(cfg, p))
            && r > 1
            && r % 2 == 1
        {
            cx.warn(
                key,
                format!("{p:?} wants a 50 % duty kernel clock: PLL ratio ÷{r} is odd"),
                limits::DUTY50_SRC,
            );
        }
        match p {
            Periph::UsbOtgHs => {
                if f != Exact::new_in::<Mega>(48, 1) {
                    cx.error(
                        key,
                        format!("USB needs exactly 48 MHz, got {f}"),
                        limits::USB_SRC,
                    );
                }
                if mux_of(cfg, p) == Ker::Hsi48 {
                    cx.warn(
                        key,
                        "HSI48 is −4.5..+3.5 % over temperature: USB needs the CRS trimming it"
                            .into(),
                        Source::Ds13311 {
                            table: 39,
                            page: 149,
                        },
                    );
                }
                if let Some(h) = t.hclk
                    && h <= limits::USB_AHB_MIN
                {
                    cx.error(
                        "sys.hpre",
                        format!("USB OTG HS needs AHB above 30 MHz, got {h}"),
                        limits::USB_SRC,
                    );
                }
            }
            Periph::I2c1235 | Periph::I2c4 => {
                // ES0491 Rev 9 2.18.1: kernel clock ≥ 4 MHz even for Standard mode
                if f < Exact::new_in::<Mega>(4, 1) {
                    cx.warn(
                        key,
                        format!("I2C kernel clock {f} below 4 MHz"),
                        Source::Es0491 { erratum: "2.18.1" },
                    );
                }
                // ES0491 Rev 9 2.18.4: APB / kernel ratio between 1.5 and 3 can stall after the first byte
                let apb = if p == Periph::I2c4 { t.pclk4 } else { t.pclk1 };
                if let Some(apb) = apb {
                    let r = (apb / f).map_or(Exact::ZERO, |r| r.as_ratio());
                    if r > Exact::new(3, 2) && r < Exact::int(3) {
                        cx.warn(
                            key,
                            format!("APB {apb} to I2C kernel {f} is a ratio between 1.5 and 3"),
                            Source::Es0491 { erratum: "2.18.4" },
                        );
                    }
                }
            }
            _ => {}
        }
    }

    // targets hold at the nominal FRACN; steering moves a clock on purpose, so the steer ends check limits only
    if !cx.at.is_empty() {
        return;
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

/// The FLASH_ACR wait states for an AXI clock at a VOS (RM Table 16); `None` above the table.
pub fn flash_wait_states(hclk: Exact<Hertz>, vos: Vos) -> Option<u8> {
    limits::FLASH_WS[vos.idx()]
        .iter()
        .position(|top| top.is_some_and(|t| hclk <= t))
        .map(|w| w as u8)
}
