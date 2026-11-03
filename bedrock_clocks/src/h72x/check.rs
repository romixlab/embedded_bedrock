//! The checker: every finding at once, each with the config key, the exact value, the allowed range and the source.

use core::fmt;

use super::config::{
    Config, HseMode, Ker, Line, Package, Periph, PllSource, Supply, SysSource, TempRange, Vos,
};
use super::limits::{self, PerVos, Range};
use super::tree::{self, Tree};
use crate::ratio::{Hz, Ratio};
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

/// A FRACN steering range turned into register values, both ends included.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct FracnSpan {
    pub nominal: u16,
    pub min: u16,
    pub max: u16,
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

    fn in_range(&mut self, key: &str, what: &str, f: Hz, r: &Range) {
        if f < r.min || f > r.max {
            self.error(
                key,
                format!("{what} {f} outside {}..={}", r.min, r.max),
                r.source,
            );
        }
    }

    fn at_most(&mut self, key: &str, what: &str, f: Hz, lim: &PerVos, vos: Vos) {
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

/// Ratios D1CPRE and HPRE can divide by (RM §8.7.7: no ÷32).
pub const HPRE_RATIOS: [u16; 9] = [1, 2, 4, 8, 16, 64, 128, 256, 512];
/// Ratios the APB prescalers can divide by (RM §8.7.7-8.7.9).
pub const PPRE_RATIOS: [u8; 5] = [1, 2, 4, 8, 16];

/// FRACN at the ends of a steer range: the multiplier N + FRACN/8192 scaled by (1 + ppm/10⁶), rounded inwards so
/// the end stays inside the range. `None` when the PLL has no FRACN or no steer range.
pub fn fracn_span(
    n: u16,
    fracn: Option<u16>,
    steer: Option<super::config::SteerPpm>,
) -> Option<(i64, i64, u16)> {
    let (f, s) = (fracn?, steer?);
    let total = n as i128 * 8192 + f as i128; // the multiplier in 1/8192 steps
    // total·(1 + ppm/1e6) - n·8192, in FRACN steps
    let end = |ppm: i32, up: bool| -> i64 {
        let num = total * (1_000_000 + ppm as i128);
        let q = if up {
            num.div_euclid(1_000_000) // round towards the nominal: down for the top end
        } else {
            -((-num).div_euclid(1_000_000)) // ceil for the bottom end
        };
        (q - n as i128 * 8192) as i64
    };
    Some((end(s.min, false), end(s.max, true), f))
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
        let Some((lo, hi, _)) = fracn_span(p.n, p.fracn, p.steer) else {
            continue;
        };
        let key = format!("pll{}.steer", i + 1);
        let s = p.steer.unwrap();
        for (ppm, fr) in [(s.min, lo), (s.max, hi)] {
            if !(1..=limits::FRACN_MAX as i64).contains(&fr) {
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
            }
            let mut c2 = cfg.clone();
            if let Some(pp) = c2.pll[i].as_mut() {
                pp.fracn = Some(fr as u16);
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
        let min = limits::VOS0_VDD_MIN_MV
            .iter()
            .find(|(k, _)| *k == kind)
            .map(|(_, v)| *v)
            .unwrap();
        if cfg.vdd_mv < min {
            cx.error(
                "vdd",
                format!(
                    "VOS0 with a {kind} supply needs VDD ≥ {} mV, the board has {} mV",
                    min, cfg.vdd_mv
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
    if cfg.vdd_mv < 2700 && cfg.vos <= Vos::Vos1 {
        cx.warn(
            "vdd",
            format!(
                "the WWDG doesn't work at {} mV with {:?}",
                cfg.vdd_mv, cfg.vos
            ),
            Source::Es0491 { erratum: "2.16.1" },
        );
    }

    // oscillators
    if let Some(h) = cfg.hse {
        cx.in_range("hse.freq", "HSE", h.freq, &limits::HSE);
        let _ = h.mode == HseMode::Bypass;
    }
    if let Some(d) = cfg.hsi_div
        && !matches!(d, 1 | 2 | 4 | 8)
    {
        cx.error(
            "hsi.div",
            format!("HSIDIV ÷{d} doesn't exist (1, 2, 4, 8)"),
            limits::HSI_SRC,
        );
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
        let (lo, hi, src) = limits::DIVM;
        if !(lo..=hi).contains(&(p.m as u32)) {
            cx.error(k("m"), format!("DIVM ÷{} outside {lo}..={hi}", p.m), src);
        }
        let (lo, hi, src) = limits::DIVN;
        if !(lo..=hi).contains(&(p.n as u32)) {
            cx.error(k("n"), format!("DIVN ×{} outside {lo}..={hi}", p.n), src);
        }
        for (name, v) in [("p", p.p), ("q", p.q), ("r", p.r)] {
            let (lo, hi, src) = limits::DIVPQR;
            if let Some(v) = v
                && !(lo..=hi).contains(&(v as u32))
            {
                cx.error(k(name), format!("÷{v} outside {lo}..={hi}"), src);
            }
        }
        if i == 0
            && let Some(pd) = p.p
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
        match p.fracn {
            Some(0) => cx.error(
                k("fracn"),
                "FRACN 0 is integer mode in hardware (SH_REG = 0): leave fracn out instead".into(),
                limits::FRACN_SRC,
            ),
            Some(f) if f as u32 > limits::FRACN_MAX => cx.error(
                k("fracn"),
                format!("FRACN {f} above {}", limits::FRACN_MAX),
                limits::FRACN_SRC,
            ),
            _ => {}
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
                    format!("steer range {}..{} ppm is empty", s.min, s.max),
                    limits::FRACN_SRC,
                );
            }
        }
    }

    // bus prescalers
    let s = &cfg.sys;
    for (key, v) in [("sys.d1cpre", s.d1cpre), ("sys.hpre", s.hpre)] {
        if !HPRE_RATIOS.contains(&v) {
            cx.error(
                key,
                format!("÷{v} doesn't exist (1, 2, 4, 8, 16, 64, 128, 256, 512; no ÷32)"),
                limits::HPRE_SRC,
            );
        }
    }
    for (key, v) in [
        ("sys.d1ppre", s.d1ppre),
        ("sys.d2ppre1", s.d2ppre1),
        ("sys.d2ppre2", s.d2ppre2),
        ("sys.d3ppre", s.d3ppre),
    ] {
        if !PPRE_RATIOS.contains(&v) {
            cx.error(
                key,
                format!("÷{v} doesn't exist (1, 2, 4, 8, 16)"),
                limits::HPRE_SRC,
            );
        }
    }
    if s.source == SysSource::Pll1P && cfg.pll[0].and_then(|p| p.p).is_none() {
        cx.error(
            "sys.source",
            "sys_ck from PLL1 P, but PLL1 P is off".into(),
            limits::FRACN_SRC,
        );
    }

    // kernel mux choices (RM §8.7.18-8.7.21)
    use Ker::*;
    let k = &cfg.kernel;
    let allowed: [(&str, Ker, &[Ker]); 15] = [
        ("kernel.per_ck", k.per_ck, &[HsiKer, CsiKer, Hse, Disabled]),
        ("kernel.sdmmc", k.sdmmc, &[Pll1Q, Pll2R]),
        ("kernel.octospi", k.octospi, &[Bus, Pll1Q, Pll2R, PerCk]),
        ("kernel.fmc", k.fmc, &[Bus, Pll1Q, Pll2R, PerCk]),
        ("kernel.fdcan", k.fdcan, &[Hse, Pll1Q, Pll2Q, Disabled]),
        (
            "kernel.spi45",
            k.spi45,
            &[Bus, Pll2Q, Pll3Q, HsiKer, CsiKer, Hse],
        ),
        ("kernel.spi123", k.spi123, &[Pll1Q, Pll2P, Pll3P, PerCk]),
        ("kernel.usb", k.usb, &[Disabled, Pll1Q, Pll3Q, Hsi48]),
        ("kernel.i2c1235", k.i2c1235, &[Bus, Pll3R, HsiKer, CsiKer]),
        (
            "kernel.usart16910",
            k.usart16910,
            &[Bus, Pll2Q, Pll3Q, HsiKer, CsiKer, Lse],
        ),
        (
            "kernel.usart234578",
            k.usart234578,
            &[Bus, Pll2Q, Pll3Q, HsiKer, CsiKer, Lse],
        ),
        (
            "kernel.spi6",
            k.spi6,
            &[Bus, Pll2Q, Pll3Q, HsiKer, CsiKer, Hse],
        ),
        ("kernel.adc", k.adc, &[Pll2P, Pll3R, PerCk]),
        ("kernel.i2c4", k.i2c4, &[Bus, Pll3R, HsiKer, CsiKer]),
        (
            "kernel.lpuart1",
            k.lpuart1,
            &[Bus, Pll2Q, Pll3Q, HsiKer, CsiKer, Lse],
        ),
    ];
    for (key, v, ok) in allowed {
        if !ok.contains(&v) {
            cx.error(
                key,
                format!("this mux can't select {v:?} (it can: {ok:?})"),
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
                if f != Hz::mhz(48) {
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
                if f < Hz::mhz(4) {
                    cx.warn(
                        key,
                        format!("I2C kernel clock {f} below 4 MHz"),
                        Source::Es0491 { erratum: "2.18.1" },
                    );
                }
                // ES0491 Rev 9 2.18.4: APB / kernel ratio between 1.5 and 3 can stall after the first byte
                let apb = if p == Periph::I2c4 { t.pclk4 } else { t.pclk1 };
                if let Some(apb) = apb {
                    let r = apb.0.checked_div(f.0).unwrap_or(Ratio::ZERO);
                    if r > Ratio::new(3, 2) && r < Ratio::int(3) {
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
        if e.tolerance_ppm == 0 {
            if f != e.freq {
                cx.push(
                    Severity::Error,
                    key,
                    format!("{f}, not exactly {}", e.freq),
                    None,
                );
            }
        } else {
            // |f - target| ≤ target·tol/1e6
            let diff =
                f.0.checked_sub(e.freq.0)
                    .ok()
                    .flatten()
                    .or_else(|| e.freq.0.checked_sub(f.0).ok().flatten());
            let allowed = e
                .freq
                .0
                .mul_int(e.tolerance_ppm as u128)
                .and_then(|a| a.div_int(1_000_000));
            if let (Some(d), Ok(a)) = (diff, allowed)
                && d > a
            {
                cx.push(
                    Severity::Error,
                    key,
                    format!("{f} is off {} by more than {} ppm", e.freq, e.tolerance_ppm),
                    None,
                );
            }
        }
    }
}

/// The FLASH_ACR wait states for an AXI clock at a VOS (RM Table 16); `None` above the table.
pub fn flash_wait_states(hclk: Hz, vos: Vos) -> Option<u8> {
    limits::FLASH_WS[vos.idx()]
        .iter()
        .position(|top| top.is_some_and(|t| hclk <= t))
        .map(|w| w as u8)
}
