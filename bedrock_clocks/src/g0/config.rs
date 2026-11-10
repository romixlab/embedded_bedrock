//! The input: what the board has and what the firmware asks for. Plain data; the TOML front end (RCC-2) and Rust
//! code both build it.

use exact_units::prefix::One;
use exact_units::units::{Hertz, Ratio};
use exact_units::{Exact, Fixed, Range, Tolerance};

use super::limits;
use crate::knob::KnobError;

/// The RM0444 lines with HSI48, USB and the second APB peripherals (RM0444 Rev 5 §1.4).
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum Line {
    G0B1,
    /// G0B1 with AES (errata ES0549; the clock tree is the same).
    G0C1,
}

/// Package letter of the part number (DS13560 Rev 6 §7 "Ordering information").
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum Package {
    /// T
    Lqfp,
    /// U
    Ufqfpn,
    /// I
    Ufbga,
    /// Y
    Wlcsp,
}

/// A part number like `STM32G0B1CEU6`, decoded.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Part {
    pub name: String,
    pub line: Line,
    /// Pin count letter: K 32, C 48, R 64, M 80, V 100.
    pub pins: char,
    /// Flash in KiB: B 128, C 256, E 512.
    pub flash_kib: u16,
    pub package: Package,
    /// Temperature range digit: 6, 7 or 3.
    pub temp: char,
}

impl Part {
    /// Decodes `STM32G0[BC]1PFKT`: pins, flash size, package and temperature range (DS13560 Rev 6 §7).
    pub fn parse(s: &str) -> Result<Part, String> {
        let up = s.trim().to_ascii_uppercase();
        let b = up.as_bytes();
        if b.len() < 13 || !up.starts_with("STM32G0") {
            return Err(format!(
                "`{s}` is not an STM32G0B1 / G0C1 part number like STM32G0B1CEU6"
            ));
        }
        let line = match &up[5..9] {
            "G0B1" => Line::G0B1,
            "G0C1" => Line::G0C1,
            other => return Err(format!("`{other}` is not a G0B1 or G0C1")),
        };
        let pins = b[9] as char;
        if !"KCRMV".contains(pins) {
            return Err(format!("pin count letter `{pins}` unknown (K, C, R, M, V)"));
        }
        let flash_kib = match b[10] {
            b'B' => 128,
            b'C' => 256,
            b'E' => 512,
            c => {
                return Err(format!(
                    "flash size letter `{}` unknown (B, C, E)",
                    c as char
                ));
            }
        };
        let package = match b[11] {
            b'T' => Package::Lqfp,
            b'U' => Package::Ufqfpn,
            b'I' => Package::Ufbga,
            b'Y' => Package::Wlcsp,
            c => {
                return Err(format!(
                    "package letter `{}` unknown (T, U, I, Y)",
                    c as char
                ));
            }
        };
        let temp = b[12] as char;
        if !"673".contains(temp) {
            return Err(format!(
                "temperature range digit `{temp}` unknown (6, 7, 3)"
            ));
        }
        Ok(Part {
            name: up[..13].to_string(),
            line,
            pins,
            flash_kib,
            package,
            temp,
        })
    }

    /// Whether the flash runs in dual-bank mode: always for 512 KiB, by the DUAL_BANK option bit for 256 KiB, never
    /// for 128 KiB (ES0548 Rev 4 2.2.10).
    pub const fn dual_bank(&self, dual_bank_option: bool) -> bool {
        match self.flash_kib {
            512 => true,
            256 => dual_bank_option,
            _ => false,
        }
    }
}

/// Voltage scaling, PWR_CR1 VOS (RM0444 §4.1.4, §4.4.1).
#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Vos {
    /// High performance, VCORE 1.2 V typical, SYSCLK up to 64 MHz (the reset value).
    Range1,
    /// Low power, VCORE 1.0 V typical, SYSCLK up to 16 MHz, no flash write or erase.
    Range2,
}

impl Vos {
    /// Column index into the per-range limit tables: Range 1 first.
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

/// What trims HSI48 (CRS_CFGR SYNCSRC, RM0444 §6.7.2): only the USB SOF is modelled.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum CrsSync {
    /// The 1 kHz USB start of frame (crs_sync_in_3, RM0444 Table 38).
    UsbSof,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct Hsi48 {
    /// `Some`: the CRS trims HSI48 from this SYNC source, as USB needs (RM0444 §5.2.3).
    pub crs: Option<CrsSync>,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum PllSource {
    Hsi16,
    Hse,
}

/// PLLM as a ratio, ÷1..÷8 (RM0444 §5.4.4 p.185).
pub type Pllm = Fixed<Ratio, One, u8, Range<1, 8>>;
/// PLLN as a ratio, ×8..×86 (RM0444 §5.4.4 p.185).
pub type Plln = Fixed<Ratio, One, u8, Range<8, 86>>;
/// PLLP as a ratio, ÷2..÷32 (RM0444 §5.4.4 p.185).
pub type Pllp = Fixed<Ratio, One, u8, Range<2, 32>>;
/// PLLQ / PLLR as a ratio, ÷2..÷8 (RM0444 §5.4.4 p.184).
pub type Pllqr = Fixed<Ratio, One, u8, Range<2, 8>>;

/// A knob value outside its field's range, in exact_units' words after the knob's: `PLLM ÷9 is out of 1..=8 (Ratio,
/// u8)`.
fn out_of_range(field: &'static str, what: &str, e: impl core::fmt::Display) -> KnobError {
    KnobError {
        field,
        message: format!("{what}{e}"),
        source: limits::PLLCFGR_SRC,
    }
}

pub fn pllm(m: u8) -> Result<Pllm, KnobError> {
    Pllm::try_new(m).map_err(|e| out_of_range("m", "PLLM ÷", e))
}

pub fn plln(n: u8) -> Result<Plln, KnobError> {
    Plln::try_new(n).map_err(|e| out_of_range("n", "PLLN ×", e))
}

pub fn pllp(p: u8) -> Result<Pllp, KnobError> {
    Pllp::try_new(p).map_err(|e| out_of_range("p", "PLLP ÷", e))
}

/// `field` is `q` or `r`.
pub fn pllqr(field: &'static str, d: u8) -> Result<Pllqr, KnobError> {
    Pllqr::try_new(d).map_err(|e| out_of_range(field, "÷", e))
}

/// The one PLL. Dividers are ratios as the RM writes them, not register values.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct Pll {
    pub source: PllSource,
    pub m: Pllm,
    pub n: Plln,
    pub p: Option<Pllp>,
    pub q: Option<Pllqr>,
    pub r: Option<Pllqr>,
}

impl Pll {
    /// A PLL from plain ratios, each checked by its knob type.
    pub fn from_ratios(
        source: PllSource,
        m: u8,
        n: u8,
        p: Option<u8>,
        q: Option<u8>,
        r: Option<u8>,
    ) -> Result<Pll, KnobError> {
        Ok(Pll {
            source,
            m: pllm(m)?,
            n: plln(n)?,
            p: p.map(pllp).transpose()?,
            q: q.map(|d| pllqr("q", d)).transpose()?,
            r: r.map(|d| pllqr("r", d)).transpose()?,
        })
    }
}

/// HSIDIV: HSI16 ÷ 1, 2, 4 ... 128 into HSISYS (RM0444 §5.4.1 p.179).
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct HsiDiv(u8);

impl HsiDiv {
    pub const DIV1: HsiDiv = HsiDiv(0);

    pub fn from_ratio(d: u8) -> Result<HsiDiv, KnobError> {
        if d.is_power_of_two() {
            Ok(HsiDiv(d.trailing_zeros() as u8))
        } else {
            Err(KnobError {
                field: "div",
                message: format!("HSIDIV ÷{d} doesn't exist (1, 2, 4 ... 128)"),
                source: limits::HSIDIV_SRC,
            })
        }
    }

    pub const fn ratio(self) -> u8 {
        1 << self.0
    }

    /// The HSIDIV field value.
    pub const fn bits(self) -> u8 {
        self.0
    }
}

/// HPRE: ÷1, 2, 4 ... 512 without ÷32 (RM0444 §5.4.3 p.183).
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct Hpre(u16);

impl Hpre {
    pub const DIV1: Hpre = Hpre(1);
    const ALL: [u16; 9] = [1, 2, 4, 8, 16, 64, 128, 256, 512];

    pub fn from_ratio(r: u16) -> Result<Hpre, KnobError> {
        if Self::ALL.contains(&r) {
            Ok(Hpre(r))
        } else {
            Err(KnobError {
                field: "hpre",
                message: format!("÷{r} doesn't exist (1, 2, 4, 8, 16, 64, 128, 256, 512; no ÷32)"),
                source: limits::HPRE_SRC,
            })
        }
    }

    pub const fn ratio(self) -> u16 {
        self.0
    }

    /// The HPRE field value: 0 for ÷1, then 0b1000 + the index from ÷2.
    pub const fn bits(self) -> u8 {
        match self.0 {
            1 => 0,
            r => {
                let log = r.trailing_zeros() as u8;
                // ÷64 and up skip the missing ÷32
                0b1000 + log - 1 - if log > 5 { 1 } else { 0 }
            }
        }
    }
}

/// PPRE: ÷1, 2, 4, 8 or 16 (RM0444 §5.4.3 p.182).
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct Ppre(u8);

impl Ppre {
    pub const DIV1: Ppre = Ppre(1);

    pub fn from_ratio(r: u8) -> Result<Ppre, KnobError> {
        if r.is_power_of_two() && r <= 16 {
            Ok(Ppre(r))
        } else {
            Err(KnobError {
                field: "ppre",
                message: format!("÷{r} doesn't exist (1, 2, 4, 8, 16)"),
                source: limits::PPRE_SRC,
            })
        }
    }

    pub const fn ratio(self) -> u8 {
        self.0
    }

    /// The PPRE field value: 0 for ÷1, then 0b100 + the index from ÷2.
    pub const fn bits(self) -> u8 {
        match self.0 {
            1 => 0,
            r => 0b100 + r.trailing_zeros() as u8 - 1,
        }
    }
}

/// The SYSCLK sources the planner models (RM0444 §5.4.3 SW; LSI and LSE as SYSCLK are not).
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum SysSource {
    Hsisys,
    Hse,
    PllR,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct Sys {
    pub source: SysSource,
    pub hpre: Hpre,
    pub ppre: Ppre,
}

/// Kernel clock sources a mux can pick (RM0444 §5.4.21-5.4.22). Each mux allows a subset; the checker refuses the
/// others.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum Ker {
    Sysclk,
    Pclk,
    /// TIMPCLK: PCLK, or 2 × PCLK when PPRE divides (RM0444 §5.2 p.163).
    Timpclk,
    Hsi16,
    /// HSI16 ÷ 8, the RNG's choice.
    Hsi16Div8,
    Hsi48,
    Hse,
    Lsi,
    Lse,
    PllP,
    PllQ,
    /// The I2S_CKIN pin, not modelled: a peripheral in use on it has no known clock.
    I2sCkin,
    Disabled,
}

/// The kernel clock muxes of RCC_CCIPR and RCC_CCIPR2 (CEC, HSI16 ÷ 488 or LSE, not modelled). A field at its
/// default keeps the reset value 0 of its register.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct Kernel {
    /// CCIPR ADCSEL: Sysclk (reset), PllP, Hsi16.
    pub adc: Ker,
    /// CCIPR RNGSEL: Disabled (reset), Hsi16Div8, Sysclk, PllQ; then ÷ [Kernel::rng_div].
    pub rng: Ker,
    /// CCIPR RNGDIV: 1 (reset), 2, 4, 8.
    pub rng_div: u8,
    /// CCIPR TIM15SEL: Timpclk (reset), PllQ.
    pub tim15: Ker,
    /// CCIPR TIM1SEL: Timpclk (reset), PllQ.
    pub tim1: Ker,
    /// CCIPR LPTIM2SEL: Pclk (reset), Lsi, Hsi16, Lse.
    pub lptim2: Ker,
    /// CCIPR LPTIM1SEL: same choices.
    pub lptim1: Ker,
    /// CCIPR I2C2I2S1SEL (I2C2 on the G0B1 / G0C1): Pclk (reset), Sysclk, Hsi16.
    pub i2c2: Ker,
    /// CCIPR I2C1SEL: same choices.
    pub i2c1: Ker,
    /// CCIPR LPUART1SEL: Pclk (reset), Sysclk, Hsi16, Lse.
    pub lpuart1: Ker,
    /// CCIPR LPUART2SEL: same choices.
    pub lpuart2: Ker,
    /// CCIPR USART3SEL: same choices.
    pub usart3: Ker,
    /// CCIPR USART2SEL: same choices.
    pub usart2: Ker,
    /// CCIPR USART1SEL: same choices.
    pub usart1: Ker,
    /// CCIPR2 USBSEL: Hsi48 (reset), PllQ, Hse.
    pub usb: Ker,
    /// CCIPR2 FDCANSEL: Pclk (reset), PllQ, Hse.
    pub fdcan: Ker,
    /// CCIPR2 I2S2SEL: Sysclk (reset), PllP, Hsi16, I2sCkin.
    pub i2s2: Ker,
    /// CCIPR2 I2S1SEL: same choices.
    pub i2s1: Ker,
}

impl Default for Kernel {
    /// The reset values of RM0444 §5.4.21-5.4.22 (both registers reset to 0).
    fn default() -> Self {
        Kernel {
            adc: Ker::Sysclk,
            rng: Ker::Disabled,
            rng_div: 1,
            tim15: Ker::Timpclk,
            tim1: Ker::Timpclk,
            lptim2: Ker::Pclk,
            lptim1: Ker::Pclk,
            i2c2: Ker::Pclk,
            i2c1: Ker::Pclk,
            lpuart1: Ker::Pclk,
            lpuart2: Ker::Pclk,
            usart3: Ker::Pclk,
            usart2: Ker::Pclk,
            usart1: Ker::Pclk,
            usb: Ker::Hsi48,
            fdcan: Ker::Pclk,
            i2s2: Ker::Sysclk,
            i2s1: Ker::Sysclk,
        }
    }
}

/// Peripherals whose kernel clock the firmware uses: their clock must exist and be within its limit.
#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Periph {
    Adc,
    Rng,
    Tim15,
    Tim1,
    Lptim2,
    Lptim1,
    I2c2,
    I2c1,
    Lpuart1,
    Lpuart2,
    Usart3,
    Usart2,
    Usart1,
    Usb,
    Fdcan,
    I2s2,
    I2s1,
    /// UCPD1 / UCPD2: no mux, always HSI16 (RM0444 §5.2 p.164).
    Ucpd,
}

impl Periph {
    pub const ALL: [Periph; 18] = [
        Periph::Adc,
        Periph::Rng,
        Periph::Tim15,
        Periph::Tim1,
        Periph::Lptim2,
        Periph::Lptim1,
        Periph::I2c2,
        Periph::I2c1,
        Periph::Lpuart1,
        Periph::Lpuart2,
        Periph::Usart3,
        Periph::Usart2,
        Periph::Usart1,
        Periph::Usb,
        Periph::Fdcan,
        Periph::I2s2,
        Periph::I2s1,
        Periph::Ucpd,
    ];

    /// The config key of its mux.
    pub const fn key(self) -> &'static str {
        match self {
            Periph::Adc => "kernel.adc",
            Periph::Rng => "kernel.rng",
            Periph::Tim15 => "kernel.tim15",
            Periph::Tim1 => "kernel.tim1",
            Periph::Lptim2 => "kernel.lptim2",
            Periph::Lptim1 => "kernel.lptim1",
            Periph::I2c2 => "kernel.i2c2",
            Periph::I2c1 => "kernel.i2c1",
            Periph::Lpuart1 => "kernel.lpuart1",
            Periph::Lpuart2 => "kernel.lpuart2",
            Periph::Usart3 => "kernel.usart3",
            Periph::Usart2 => "kernel.usart2",
            Periph::Usart1 => "kernel.usart1",
            Periph::Usb => "kernel.usb",
            Periph::Fdcan => "kernel.fdcan",
            Periph::I2s2 => "kernel.i2s2",
            Periph::I2s1 => "kernel.i2s1",
            Periph::Ucpd => "kernel.ucpd",
        }
    }
}

/// A target a clock must meet within a tolerance either way: `Tolerance::EXACT`, or a `q!(+- 1 %)` literal.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct Expect {
    pub periph: Periph,
    pub freq: Exact<Hertz>,
    pub tolerance: Tolerance,
}

/// Findings a board accepts on purpose, each by name.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
pub struct Allow {
    /// Flash prefetch on a dual-bank part: ES0548 Rev 4 2.2.10, no workaround. Only for code that never branches
    /// across the banks (a firmware that fits bank 1, bank 2 holding data or a second image).
    pub prefetch_dual_bank: bool,
}

/// The whole input.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Config {
    pub part: Part,
    pub vos: Vos,
    /// The DUAL_BANK option bit (only read for 256 KiB parts; 512 KiB parts are always dual-bank).
    pub dual_bank_option: bool,
    /// FLASH_ACR PRFTEN.
    pub prefetch: bool,
    /// FLASH_ACR ICEN (on at reset).
    pub icache: bool,
    pub hse: Option<Hse>,
    /// HSI16 on, with its divider into HSISYS. Off is refused while HSISYS, the PLL, a kernel mux or UCPD needs it.
    pub hsi16: Option<HsiDiv>,
    pub hsi48: Option<Hsi48>,
    /// LSE frequency when fitted and used as a kernel clock (the RTC domain is outside this planner).
    pub lse: Option<Exact<Hertz>>,
    pub pll: Option<Pll>,
    pub sys: Sys,
    pub kernel: Kernel,
    pub used: Vec<Periph>,
    pub expect: Vec<Expect>,
    pub allow: Allow,
}
