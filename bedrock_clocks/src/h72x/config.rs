//! The input: what the board has and what the firmware asks for. Plain data; the TOML front end (RCC-2) and Rust
//! code both build it.

use crate::ratio::Hz;

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
    pub freq: Hz,
    pub mode: HseMode,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum PllSource {
    Hsi,
    Csi,
    Hse,
}

/// A FRACN steering range in ppm of the nominal VCO, both ends included: `(-100, 100)`.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct SteerPpm {
    pub min: i32,
    pub max: i32,
}

/// One PLL. Dividers are ratios as the RM writes them (`n = 173` means ×173), not register values.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct Pll {
    pub m: u8,
    pub n: u16,
    /// `Some` puts the PLL in fractional mode (PLLxFRACEN = 1). `Some(0)` is integer mode in hardware (SH_REG = 0)
    /// and is refused.
    pub fracn: Option<u16>,
    pub p: Option<u8>,
    pub q: Option<u8>,
    pub r: Option<u8>,
    /// `Some` when firmware steers FRACN at run time: the checks run at both ends.
    pub steer: Option<SteerPpm>,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum SysSource {
    Hsi,
    Csi,
    Hse,
    Pll1P,
}

/// Bus prescalers as ratios (`hpre = 2` is ÷2).
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct Sys {
    pub source: SysSource,
    pub d1cpre: u16,
    pub hpre: u16,
    pub d1ppre: u8,
    pub d2ppre1: u8,
    pub d2ppre2: u8,
    pub d3ppre: u8,
    pub timpre: bool,
}

impl Default for Sys {
    fn default() -> Self {
        Sys {
            source: SysSource::Hsi,
            d1cpre: 1,
            hpre: 1,
            d1ppre: 1,
            d2ppre1: 1,
            d2ppre2: 1,
            d3ppre: 1,
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

/// A target a clock must meet: exactly, or within `tolerance_ppm` either way.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct Expect {
    pub periph: Periph,
    pub freq: Hz,
    /// 0: exact.
    pub tolerance_ppm: u32,
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
    /// VDD in mV.
    pub vdd_mv: u32,
    pub supply: Supply,
    pub vos: Vos,
    /// CPUFREQ_BOOST option byte set: 550 MHz allowed, TCM ECC off (RM0468 §4.9.25).
    pub cpu_boost: bool,
    pub hse: Option<Hse>,
    /// HSI on; its divider HSIDIV as a ratio 1, 2, 4 or 8 (64 MHz / div).
    pub hsi_div: Option<u8>,
    pub csi: bool,
    pub hsi48: bool,
    /// LSE crystal frequency when fitted and used as a kernel clock (the RTC domain is outside this planner).
    pub lse: Option<Hz>,
    pub pll_source: PllSource,
    pub pll: [Option<Pll>; 3],
    pub sys: Sys,
    pub kernel: Kernel,
    pub used: Vec<Periph>,
    pub expect: Vec<Expect>,
    pub allow: Allow,
}
