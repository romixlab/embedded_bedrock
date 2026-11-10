//! The RM0444 G0B1 / G0C1 limits, each with the document location it was read from. Per-range tables are indexed by
//! [Vos::idx](super::config::Vos::idx): Range 1, Range 2.
//!
//! Where two documents disagree the smaller limit is taken and both are named: the VCO minimum (RM 64 MHz, DS
//! 96 MHz), PLLQ in Range 2 (RM 32 MHz, DS 33 MHz) and HSE in Range 2 (RM 16 MHz, DS 26 MHz for a bypass clock).

use exact_units::Exact;
use exact_units::prefix::{Kilo, Mega};
use exact_units::units::Hertz;

use crate::source::Source;

/// `n` MHz.
const fn mhz(n: i128) -> Exact<Hertz> {
    Exact::new_in::<Mega>(n, 1)
}

/// A closed range of frequencies with its source.
#[derive(Copy, Clone, Debug)]
pub struct Range {
    pub min: Exact<Hertz>,
    pub max: Exact<Hertz>,
    pub source: Source,
}

/// A per-range minimum and maximum with its source.
#[derive(Copy, Clone, Debug)]
pub struct PerRange {
    pub min: Exact<Hertz>,
    pub max: [Exact<Hertz>; 2],
    pub source: Source,
}

// --- Oscillators

/// HSE crystal 4 to 48 MHz (RM §5.2.1 p.168; DS Table 42 p.83 the same).
pub const HSE_CRYSTAL: Range = Range {
    min: mhz(4),
    max: mhz(48),
    source: Source::Ds13560 {
        table: 42,
        page: 83,
    },
};
/// HSE bypass: an external clock of 8 to 48 MHz in Range 1 (DS Table 40 p.81; RM §5.2.1 p.168 says "up to 48 MHz").
pub const HSE_BYPASS: Range = Range {
    min: mhz(8),
    max: mhz(48),
    source: Source::Ds13560 {
        table: 40,
        page: 81,
    },
};
/// HSE in Range 2: 16 MHz (RM Table 35 p.171; DS Table 40 says 26 MHz for a bypass clock).
pub const HSE_RANGE2_MAX: Exact<Hertz> = mhz(16);
pub const CLOCK_PER_RANGE_SRC: Source = Source::Rm0444Table {
    table: 35,
    page: 171,
};

/// HSI16 nominal (RM §5.2.2 p.168; DS Table 44 p.86: 15.88 to 16.08 MHz at 30 °C, ±1 % 0 to 85 °C).
pub const HSI16: Exact<Hertz> = mhz(16);
pub const HSIDIV_SRC: Source = Source::Rm0444 {
    section: "5.4.1",
    page: 179,
};
/// HSI48 nominal (RM §5.2.3 p.169; DS Table 45 p.87: ±3 % −15 to 85 °C without the CRS); no HSI48 in Range 2 (RM
/// Table 35).
pub const HSI48: Exact<Hertz> = mhz(48);
pub const HSI48_SRC: Source = Source::Rm0444 {
    section: "5.2.3",
    page: 169,
};
/// LSI nominal, 32 kHz (RM §5.2.6 p.170).
pub const LSI: Exact<Hertz> = Exact::new_in::<Kilo>(32, 1);

// --- PLL

/// PLL input after PLLM, 2.66 to 16 MHz (RM §5.2.4 p.169, DS Table 47 p.88).
pub const PLL_IN: Range = Range {
    min: Exact::new_in::<Mega>(266, 100),
    max: mhz(16),
    source: Source::Rm0444 {
        section: "5.2.4",
        page: 169,
    },
};
/// PLLM / PLLN / PLLP / PLLQ / PLLR field ranges: the knob types (RM §5.4.4 p.184-185).
pub const PLLCFGR_SRC: Source = Source::Rm0444 {
    section: "5.4.4",
    page: 185,
};

const TABLE47: Source = Source::Ds13560 {
    table: 47,
    page: 88,
};
/// VCO 96 to 344 MHz in Range 1, to 128 MHz in Range 2 (DS Table 47 p.88; RM §5.4.4 p.185 says 64 to 344 MHz, RM
/// Table 35 the same maxima).
pub const VCO: PerRange = PerRange {
    min: mhz(96),
    max: [mhz(344), mhz(128)],
    source: TABLE47,
};
/// PLLPCLK 3.09 to 122 / 40 MHz (DS Table 47; RM Table 35 the same maxima).
pub const PLL_P: PerRange = PerRange {
    min: Exact::new_in::<Mega>(309, 100),
    max: [mhz(122), mhz(40)],
    source: TABLE47,
};
/// PLLQCLK 12 to 128 / 32 MHz (DS Table 47 says 33 MHz in Range 2, RM Table 35 32 MHz).
pub const PLL_Q: PerRange = PerRange {
    min: mhz(12),
    max: [mhz(128), mhz(32)],
    source: TABLE47,
};
/// PLLRCLK 12 to 64 / 16 MHz (DS Table 47; RM Table 35 the same maxima).
pub const PLL_R: PerRange = PerRange {
    min: mhz(12),
    max: [mhz(64), mhz(16)],
    source: TABLE47,
};

// --- Buses

/// SYSCLK up to 64 MHz in Range 1, 16 MHz in Range 2 (RM §4.1.4 p.123, §5.2.7 p.171).
pub const SYSCLK: PerRange = PerRange {
    min: Exact::ZERO,
    max: [mhz(64), mhz(16)],
    source: Source::Rm0444 {
        section: "4.1.4",
        page: 123,
    },
};
/// fHCLK and fPCLK 0 to 64 MHz (DS Table 24 p.67; RM §5.2 p.163); Range 2 follows from SYSCLK.
pub const HCLK_PCLK: Range = Range {
    min: Exact::ZERO,
    max: mhz(64),
    source: Source::Ds13560 {
        table: 24,
        page: 67,
    },
};
/// HPRE has no ÷32 (RM §5.4.3 p.183).
pub const HPRE_SRC: Source = Source::Rm0444 {
    section: "5.4.3",
    page: 183,
};
pub const PPRE_SRC: Source = Source::Rm0444 {
    section: "5.4.3",
    page: 182,
};
/// TIMPCLK is PCLK when PPRE is ÷1, else 2 × PCLK (RM §5.2 p.163).
pub const TIMPCLK_SRC: Source = Source::Rm0444 {
    section: "5.2",
    page: 163,
};

// --- Flash (RM §3.3.4 Table 12, p.72): the top of each wait-state band per range, 0 WS first.

pub const FLASH_WS: [[Option<Exact<Hertz>>; 3]; 2] = [
    // Range 1
    [Some(mhz(24)), Some(mhz(48)), Some(mhz(64))],
    // Range 2
    [Some(mhz(8)), Some(mhz(16)), None],
];
pub const FLASH_WS_SRC: Source = Source::Rm0444Table {
    table: 12,
    page: 72,
};
/// "the code prefetch may fail upon branching and function calls across flash memory banks", every revision, no
/// workaround but prefetch off (ES0548 Rev 4 2.2.10).
pub const PREFETCH_SRC: Source = Source::Es0548 { erratum: "2.2.10" };

// --- Kernel clocks

/// The ADC kernel clock "can reach 122 MHz" (RM §5.2.11 p.172), divided by ADC_CCR PRESC.
pub const ADC_KER_MAX: Exact<Hertz> = mhz(122);
pub const ADC_KER_SRC: Source = Source::Rm0444 {
    section: "5.2.11",
    page: 172,
};
/// fADC 0.14 to 35 MHz in Range 1, to 16 MHz in Range 2 (DS Table 63 p.100), after ADC_CCR PRESC.
pub const F_ADC: PerRange = PerRange {
    min: Exact::new_in::<Mega>(14, 100),
    max: [mhz(35), mhz(16)],
    source: Source::Ds13560 {
        table: 63,
        page: 100,
    },
};
/// USB: a dedicated 48 MHz clock and an APB clock of at least 12 MHz (RM §37.5.2 p.1268).
pub const USB_SRC: Source = Source::Rm0444 {
    section: "37.5.2",
    page: 1268,
};
pub const USB_PCLK_MIN: Exact<Hertz> = mhz(12);
/// UCPD is always clocked by HSI16 (RM §5.2 p.164).
pub const UCPD_SRC: Source = Source::Rm0444 {
    section: "5.2",
    page: 164,
};
/// Kernel mux choices (RM §5.4.21-5.4.22, p.209-213).
pub const MUX_SRC: Source = Source::Rm0444 {
    section: "5.4.21",
    page: 210,
};

// --- CRS (RM §6.4.3-6.4.4 p.224-225)

/// RELOAD = fTARGET / fSYNC − 1.
pub const CRS_RELOAD_SRC: Source = Source::Rm0444 {
    section: "6.4.3",
    page: 224,
};
/// FELIM = (fTARGET / fSYNC) · STEP / 2, rounded up; the reset value is for a typical step of 0.14 %.
pub const CRS_FELIM_SRC: Source = Source::Rm0444 {
    section: "6.4.4",
    page: 225,
};
/// The USB SOF, 1 kHz.
pub const USB_SOF: Exact<Hertz> = Exact::new_in::<Kilo>(1, 1);
