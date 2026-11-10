//! The input: what the board has and what the firmware asks for. Plain data; the TOML front end (RCC-2) and Rust
//! code both build it.

use exact_units::prefix::{One, Rational};
use exact_units::units::{Hertz, Ratio, Volt};
use exact_units::{Exact, Fixed, Ppm, Range, Tolerance};
use stm32_metapac::rcc::vals;

use super::limits;
use crate::source::Source;

/// The RM0468 lines.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum Line {
    H723,
    H725,
    H730,
    H733,
    H735,
}

/// Package letter of the part number (DS13311 Rev 3 §8 "Ordering information", p.279).
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum Package {
    /// T
    Lqfp,
    /// K, pitch 0.65 mm
    UfbgaP065,
    /// I, pitch 0.5 mm
    UfbgaP05,
    /// H
    Tfbga,
    /// V
    Vfqfpn,
    /// Y
    Wlcsp,
}

/// Temperature range digit (DS13311 Rev 3 §8, p.279).
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum TempRange {
    /// 6: –40 to 85 °C
    Industrial,
    /// 3: –40 to 125 °C
    Extended,
}

/// A part number like `STM32H725IGK6`, decoded.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Part {
    pub name: String,
    pub line: Line,
    /// Pin count letter: R 68, V 100/115, Z 144, A 169, I 176.
    pub pins: char,
    pub package: Package,
    pub temp: TempRange,
}

impl Part {
    /// Decodes `STM32H7xxPFKT[TR]`. Only DS13311 (the H725 datasheet) was read for the scheme; the other RM0468
    /// lines use the same letters in their ordering-information sections, which RCC-1's follow-up checks.
    pub fn parse(s: &str) -> Result<Part, String> {
        let up = s.trim().to_ascii_uppercase();
        let b = up.as_bytes();
        if b.len() < 13 || !up.starts_with("STM32H7") {
            return Err(format!(
                "`{s}` is not an STM32H72x/73x part number like STM32H725IGK6"
            ));
        }
        let line = match &up[5..9] {
            "H723" => Line::H723,
            "H725" => Line::H725,
            "H730" => Line::H730,
            "H733" => Line::H733,
            "H735" => Line::H735,
            other => {
                return Err(format!(
                    "`{other}` is not an RM0468 line (H723, H725, H730, H733, H735)"
                ));
            }
        };
        let pins = b[9] as char;
        if !"RVZAI".contains(pins) {
            return Err(format!("pin count letter `{pins}` unknown (R, V, Z, A, I)"));
        }
        let package = match b[11] {
            b'T' => Package::Lqfp,
            b'K' => Package::UfbgaP065,
            b'I' => Package::UfbgaP05,
            b'H' => Package::Tfbga,
            b'V' => Package::Vfqfpn,
            b'Y' => Package::Wlcsp,
            c => {
                return Err(format!(
                    "package letter `{}` unknown (T, K, I, H, V, Y)",
                    c as char
                ));
            }
        };
        let temp = match b[12] {
            b'6' => TempRange::Industrial,
            b'3' => TempRange::Extended,
            c => {
                return Err(format!(
                    "temperature range digit `{}` unknown (6, 3)",
                    c as char
                ));
            }
        };
        Ok(Part {
            name: up[..13].to_string(),
            line,
            pins,
            package,
            temp,
        })
    }
}

/// SMPS output level, PWR_CR3 SDLEVEL (RM0468 §6.8.4).
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum SmpsLevel {
    V1_8,
    V2_5,
}

/// The supply configurations of RM0468 Rev 3 Table 30 (p.237-238), written once into PWR_CR3 after POR.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum Supply {
    /// 0: default, LDO supplies VCORE, SMPS on.
    Default,
    /// 1: LDO supply, SMPS off.
    Ldo,
    /// 2: direct SMPS supply, LDO bypassed.
    DirectSmps,
    /// 3: SMPS supplies the LDO.
    SmpsLdo(SmpsLevel),
    /// 4: SMPS supplies an external circuit and the LDO.
    SmpsExternalLdo(SmpsLevel),
    /// 5: SMPS supplies an external circuit, LDO bypassed.
    SmpsExternalBypass(SmpsLevel),
    /// 6: SMPS off, LDO bypassed, VCORE from outside.
    Bypass,
}

/// Voltage scaling, PWR_D3CR VOS (RM0468 §6.8.6).
#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Vos {
    Vos0,
    Vos1,
    Vos2,
    Vos3,
}

impl Vos {
    /// Column index into the per-VOS limit tables: VOS0 first.
    pub const fn idx(self) -> usize {
        self as usize
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum HseMode {
    Crystal,
    /// HSEBYP: an external clock on OSC_IN.
    Bypass,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct Hse {
    pub freq: Exact<Hertz>,
    pub mode: HseMode,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum PllSource {
    Hsi,
    Csi,
    Hse,
}

/// A knob value its type can't hold, from a constructor such as [divm] or [Hpre::from_ratio]: what was asked, what
/// is allowed and where that is written. The front end adds the config key (`pll1.m`) to make it a finding.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct KnobError {
    /// The field within its table: `m`, `fracn`, `hpre`.
    pub field: &'static str,
    pub message: String,
    pub source: Source,
}

impl core::fmt::Display for KnobError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{}: {} [{}]", self.field, self.message, self.source)
    }
}

impl std::error::Error for KnobError {}

/// DIVMx as a ratio, ÷1..÷63 (RM §8.7.10 p.377); 0 (prescaler off) is an unused PLL, not a knob value.
pub type Divm = Fixed<Ratio, One, u8, Range<1, 63>>;
/// DIVNx as a ratio, ×4..×512 (RM §8.7.12 p.383).
pub type Divn = Fixed<Ratio, One, u16, Range<4, 512>>;
/// DIVPx / DIVQx / DIVRx as a ratio, ÷1..÷128 (RM §8.7.12 p.382).
pub type Divpqr = Fixed<Ratio, One, u8, Range<1, 128>>;
/// FRACNx: the fractional part of the VCO multiplier in steps of 1/8192, 1..=8191 (RM §8.7.13 p.384). 0 is integer
/// mode in hardware (SH_REG = 0, RM §8.5.5 p.328), so a PLL in integer mode has no FRACN.
pub type Fracn = Fixed<Ratio, Rational<1, 8192>, u16, Range<1, 8191>>;

/// A knob value outside its field's range, in exact_units' words after the knob's: `DIVM ÷0 is out of 1..=63 (Ratio,
/// u8)`.
fn out_of_range(
    field: &'static str,
    what: &str,
    e: impl core::fmt::Display,
    source: Source,
) -> KnobError {
    KnobError {
        field,
        message: format!("{what}{e}"),
        source,
    }
}

pub fn divm(m: u8) -> Result<Divm, KnobError> {
    Divm::try_new(m).map_err(|e| out_of_range("m", "DIVM ÷", e, limits::DIVM_SRC))
}

pub fn divn(n: u16) -> Result<Divn, KnobError> {
    Divn::try_new(n).map_err(|e| out_of_range("n", "DIVN ×", e, limits::DIVN_SRC))
}

/// `field` is `p`, `q` or `r`.
pub fn divpqr(field: &'static str, d: u8) -> Result<Divpqr, KnobError> {
    Divpqr::try_new(d).map_err(|e| out_of_range(field, "÷", e, limits::DIVPQR_SRC))
}

pub fn fracn(f: u16) -> Result<Fracn, KnobError> {
    if f == 0 {
        return Err(KnobError {
            field: "fracn",
            message: "FRACN 0 is integer mode in hardware (SH_REG = 0): leave fracn out instead"
                .into(),
            source: limits::FRACN_SRC,
        });
    }
    Fracn::try_new(f).map_err(|e| out_of_range("fracn", "FRACN ", e, limits::FRACN_SRC))
}

/// A FRACN steering range of the nominal VCO, both ends included: `(-100 ppm, 100 ppm)`.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct SteerPpm {
    pub min: Ppm<i32>,
    pub max: Ppm<i32>,
}

impl SteerPpm {
    /// ±`ppm`.
    pub const fn symmetric(ppm: i32) -> SteerPpm {
        SteerPpm {
            min: Ppm::new(-ppm),
            max: Ppm::new(ppm),
        }
    }
}

/// One PLL. Dividers are ratios as the RM writes them (`n` ×173), not register values; each type holds only the
/// ratios its register can.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct Pll {
    pub m: Divm,
    pub n: Divn,
    /// `Some` puts the PLL in fractional mode (PLLxFRACEN = 1).
    pub fracn: Option<Fracn>,
    pub p: Option<Divpqr>,
    pub q: Option<Divpqr>,
    pub r: Option<Divpqr>,
    /// `Some` when firmware steers FRACN at run time: the checks run at both ends.
    pub steer: Option<SteerPpm>,
}

impl Pll {
    /// A PLL from plain ratios, each checked by its knob type; no steering.
    pub fn from_ratios(
        m: u8,
        n: u16,
        fracn: Option<u16>,
        p: Option<u8>,
        q: Option<u8>,
        r: Option<u8>,
    ) -> Result<Pll, KnobError> {
        Ok(Pll {
            m: divm(m)?,
            n: divn(n)?,
            fracn: fracn.map(self::fracn).transpose()?,
            p: p.map(|d| divpqr("p", d)).transpose()?,
            q: q.map(|d| divpqr("q", d)).transpose()?,
            r: r.map(|d| divpqr("r", d)).transpose()?,
            steer: None,
        })
    }

    /// The VCO multiplier N + FRACN/8192, exactly.
    pub fn multiplier(&self) -> Option<Exact<Ratio>> {
        let n = Exact::from_fixed(self.n)?;
        match self.fracn {
            Some(f) => n + Exact::from_fixed(f)?,
            None => Some(n),
        }
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum SysSource {
    Hsi,
    Csi,
    Hse,
    Pll1P,
}

/// HSIDIV: 64 MHz ÷ 1, 2, 4 or 8 (RM §8.7.2 p.365).
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum HsiDiv {
    Div1,
    Div2,
    Div4,
    Div8,
}

impl HsiDiv {
    pub fn from_ratio(d: u8) -> Result<HsiDiv, KnobError> {
        Ok(match d {
            1 => HsiDiv::Div1,
            2 => HsiDiv::Div2,
            4 => HsiDiv::Div4,
            8 => HsiDiv::Div8,
            _ => {
                return Err(KnobError {
                    field: "div",
                    message: format!("HSIDIV ÷{d} doesn't exist (1, 2, 4, 8)"),
                    source: limits::HSI_SRC,
                });
            }
        })
    }

    pub const fn ratio(self) -> u8 {
        1 << self.bits().to_bits()
    }

    pub const fn bits(self) -> vals::Hsidiv {
        match self {
            HsiDiv::Div1 => vals::Hsidiv::DIV1,
            HsiDiv::Div2 => vals::Hsidiv::DIV2,
            HsiDiv::Div4 => vals::Hsidiv::DIV4,
            HsiDiv::Div8 => vals::Hsidiv::DIV8,
        }
    }
}

/// D1CPRE / HPRE: ÷1, 2, 4 ... 512 without ÷32 (RM §8.7.7 p.373-374).
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum Hpre {
    Div1,
    Div2,
    Div4,
    Div8,
    Div16,
    Div64,
    Div128,
    Div256,
    Div512,
}

impl Hpre {
    pub const ALL: [Hpre; 9] = [
        Hpre::Div1,
        Hpre::Div2,
        Hpre::Div4,
        Hpre::Div8,
        Hpre::Div16,
        Hpre::Div64,
        Hpre::Div128,
        Hpre::Div256,
        Hpre::Div512,
    ];

    /// `field` is `d1cpre` or `hpre`.
    pub fn from_ratio(field: &'static str, r: u16) -> Result<Hpre, KnobError> {
        Hpre::ALL
            .into_iter()
            .find(|h| h.ratio() == r)
            .ok_or(KnobError {
                field,
                message: format!("÷{r} doesn't exist (1, 2, 4, 8, 16, 64, 128, 256, 512; no ÷32)"),
                source: limits::HPRE_SRC,
            })
    }

    pub const fn ratio(self) -> u16 {
        match self {
            Hpre::Div1 => 1,
            Hpre::Div2 => 2,
            Hpre::Div4 => 4,
            Hpre::Div8 => 8,
            Hpre::Div16 => 16,
            Hpre::Div64 => 64,
            Hpre::Div128 => 128,
            Hpre::Div256 => 256,
            Hpre::Div512 => 512,
        }
    }

    pub const fn bits(self) -> vals::Hpre {
        match self {
            Hpre::Div1 => vals::Hpre::DIV1,
            Hpre::Div2 => vals::Hpre::DIV2,
            Hpre::Div4 => vals::Hpre::DIV4,
            Hpre::Div8 => vals::Hpre::DIV8,
            Hpre::Div16 => vals::Hpre::DIV16,
            Hpre::Div64 => vals::Hpre::DIV64,
            Hpre::Div128 => vals::Hpre::DIV128,
            Hpre::Div256 => vals::Hpre::DIV256,
            Hpre::Div512 => vals::Hpre::DIV512,
        }
    }
}

/// An APB prescaler: ÷1, 2, 4, 8 or 16 (RM §8.7.7-8.7.9).
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum Ppre {
    Div1,
    Div2,
    Div4,
    Div8,
    Div16,
}

impl Ppre {
    pub const ALL: [Ppre; 5] = [Ppre::Div1, Ppre::Div2, Ppre::Div4, Ppre::Div8, Ppre::Div16];

    /// `field` is `d1ppre`, `d2ppre1`, `d2ppre2` or `d3ppre`.
    pub fn from_ratio(field: &'static str, r: u8) -> Result<Ppre, KnobError> {
        Ppre::ALL
            .into_iter()
            .find(|p| p.ratio() == r)
            .ok_or(KnobError {
                field,
                message: format!("÷{r} doesn't exist (1, 2, 4, 8, 16)"),
                source: limits::HPRE_SRC,
            })
    }

    pub const fn ratio(self) -> u8 {
        match self {
            Ppre::Div1 => 1,
            Ppre::Div2 => 2,
            Ppre::Div4 => 4,
            Ppre::Div8 => 8,
            Ppre::Div16 => 16,
        }
    }

    pub const fn bits(self) -> vals::Ppre {
        match self {
            Ppre::Div1 => vals::Ppre::DIV1,
            Ppre::Div2 => vals::Ppre::DIV2,
            Ppre::Div4 => vals::Ppre::DIV4,
            Ppre::Div8 => vals::Ppre::DIV8,
            Ppre::Div16 => vals::Ppre::DIV16,
        }
    }
}

/// The bus prescalers.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct Sys {
    pub source: SysSource,
    pub d1cpre: Hpre,
    pub hpre: Hpre,
    pub d1ppre: Ppre,
    pub d2ppre1: Ppre,
    pub d2ppre2: Ppre,
    pub d3ppre: Ppre,
    pub timpre: bool,
}

impl Default for Sys {
    fn default() -> Self {
        Sys {
            source: SysSource::Hsi,
            d1cpre: Hpre::Div1,
            hpre: Hpre::Div1,
            d1ppre: Ppre::Div1,
            d2ppre1: Ppre::Div1,
            d2ppre2: Ppre::Div1,
            d3ppre: Ppre::Div1,
            timpre: false,
        }
    }
}

/// Kernel clock sources a mux can pick (RM0468 §8.7.18-8.7.21). Each mux allows a subset; the checker refuses the
/// others.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum Ker {
    Pll1Q,
    Pll2P,
    Pll2Q,
    Pll2R,
    Pll3P,
    Pll3Q,
    Pll3R,
    Hse,
    HsiKer,
    CsiKer,
    Hsi48,
    Lse,
    PerCk,
    /// The mux's bus clock (rcc_pclkx or rcc_hclk3).
    Bus,
    Disabled,
}

/// The kernel clock muxes the planner knows. A field left at its default keeps the reset value of the register,
/// and its clock is still checked when the peripheral is listed in [Config::used].
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct Kernel {
    /// D1CCIPR CKPERSEL: Hsi (reset), Csi, Hse, Disabled.
    pub per_ck: Ker,
    /// D1CCIPR SDMMCSEL: Pll1Q (reset), Pll2R.
    pub sdmmc: Ker,
    /// D1CCIPR OCTOSPISEL: Bus (reset, rcc_hclk3), Pll1Q, Pll2R, PerCk.
    pub octospi: Ker,
    /// D1CCIPR FMCSEL: same choices as OCTOSPISEL.
    pub fmc: Ker,
    /// D2CCIP1R FDCANSEL: Hse (reset), Pll1Q, Pll2Q, Disabled.
    pub fdcan: Ker,
    /// D2CCIP1R SPI45SEL: Bus (reset), Pll2Q, Pll3Q, HsiKer, CsiKer, Hse.
    pub spi45: Ker,
    /// D2CCIP1R SPI123SEL: Pll1Q (reset), Pll2P, Pll3P, PerCk (I2S_CKIN not modelled).
    pub spi123: Ker,
    /// D2CCIP2R USBSEL: Disabled (reset), Pll1Q, Pll3Q, Hsi48.
    pub usb: Ker,
    /// D2CCIP2R I2C1235SEL: Bus (reset), Pll3R, HsiKer, CsiKer.
    pub i2c1235: Ker,
    /// D2CCIP2R USART16910SEL: Bus (reset), Pll2Q, Pll3Q, HsiKer, CsiKer, Lse.
    pub usart16910: Ker,
    /// D2CCIP2R USART234578SEL: same choices.
    pub usart234578: Ker,
    /// D3CCIPR SPI6SEL: Bus (reset), Pll2Q, Pll3Q, HsiKer, CsiKer, Hse.
    pub spi6: Ker,
    /// D3CCIPR ADCSEL: Pll2P (reset), Pll3R, PerCk.
    pub adc: Ker,
    /// D3CCIPR I2C4SEL: Bus (reset), Pll3R, HsiKer, CsiKer.
    pub i2c4: Ker,
    /// D3CCIPR LPUART1SEL: Bus (reset), Pll2Q, Pll3Q, HsiKer, CsiKer, Lse.
    pub lpuart1: Ker,
}

impl Default for Kernel {
    /// The reset values of RM0468 §8.7.18-8.7.21.
    fn default() -> Self {
        Kernel {
            per_ck: Ker::HsiKer,
            sdmmc: Ker::Pll1Q,
            octospi: Ker::Bus,
            fmc: Ker::Bus,
            fdcan: Ker::Hse,
            spi45: Ker::Bus,
            spi123: Ker::Pll1Q,
            usb: Ker::Disabled,
            i2c1235: Ker::Bus,
            usart16910: Ker::Bus,
            usart234578: Ker::Bus,
            spi6: Ker::Bus,
            adc: Ker::Pll2P,
            i2c4: Ker::Bus,
            lpuart1: Ker::Bus,
        }
    }
}

/// Peripherals whose kernel clock the firmware uses: their clock must exist and be within its limit.
#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Periph {
    Sdmmc,
    Octospi,
    Fmc,
    Fdcan,
    Spi45,
    Spi123,
    UsbOtgHs,
    I2c1235,
    Usart16910,
    Usart234578,
    Spi6,
    Adc,
    I2c4,
    Lpuart1,
}

impl Periph {
    pub const ALL: [Periph; 14] = [
        Periph::Sdmmc,
        Periph::Octospi,
        Periph::Fmc,
        Periph::Fdcan,
        Periph::Spi45,
        Periph::Spi123,
        Periph::UsbOtgHs,
        Periph::I2c1235,
        Periph::Usart16910,
        Periph::Usart234578,
        Periph::Spi6,
        Periph::Adc,
        Periph::I2c4,
        Periph::Lpuart1,
    ];

    /// The config key of its mux.
    pub const fn key(self) -> &'static str {
        match self {
            Periph::Sdmmc => "kernel.sdmmc",
            Periph::Octospi => "kernel.octospi",
            Periph::Fmc => "kernel.fmc",
            Periph::Fdcan => "kernel.fdcan",
            Periph::Spi45 => "kernel.spi45",
            Periph::Spi123 => "kernel.spi123",
            Periph::UsbOtgHs => "kernel.usb",
            Periph::I2c1235 => "kernel.i2c1235",
            Periph::Usart16910 => "kernel.usart16910",
            Periph::Usart234578 => "kernel.usart234578",
            Periph::Spi6 => "kernel.spi6",
            Periph::Adc => "kernel.adc",
            Periph::I2c4 => "kernel.i2c4",
            Periph::Lpuart1 => "kernel.lpuart1",
        }
    }
}

/// A target a clock must meet within a tolerance either way (`Tolerance::EXACT` for exactly).
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct Expect {
    pub periph: Periph,
    pub freq: Exact<Hertz>,
    pub tolerance: Tolerance,
}

/// Warnings a board accepts on purpose, each by name.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
pub struct Allow {
    /// VOS0 limits the junction to 105 °C (DS13311 Table 14) on an extended-temperature part.
    pub vos0_on_extended_temp: bool,
    /// VOS0 at all: AN5337 Rev 4 §7 estimates 2 years at TJ 105 °C with the core busy all the time.
    pub vos0_lifetime: bool,
    /// CPUFREQ_BOOST (550 MHz), which turns the ITCM / DTCM ECC off (RM0468 §4.9.25).
    pub cpu_boost_tcm_ecc_off: bool,
}

/// The whole input.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Config {
    pub part: Part,
    pub vdd: Exact<Volt>,
    pub supply: Supply,
    pub vos: Vos,
    /// CPUFREQ_BOOST option byte set: 550 MHz allowed, TCM ECC off (RM0468 §4.9.25).
    pub cpu_boost: bool,
    pub hse: Option<Hse>,
    /// HSI on, with its divider HSIDIV.
    pub hsi_div: Option<HsiDiv>,
    pub csi: bool,
    pub hsi48: bool,
    /// LSE crystal frequency when fitted and used as a kernel clock (the RTC domain is outside this planner).
    pub lse: Option<Exact<Hertz>>,
    pub pll_source: PllSource,
    pub pll: [Option<Pll>; 3],
    pub sys: Sys,
    pub kernel: Kernel,
    pub used: Vec<Periph>,
    pub expect: Vec<Expect>,
    pub allow: Allow,
}
