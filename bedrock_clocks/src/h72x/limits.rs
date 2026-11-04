//! The RM0468 family's limits, each with the document location it was read from. Per-VOS tables are indexed by
//! [Vos::idx](super::config::Vos::idx): VOS0, VOS1, VOS2, VOS3.
//!
//! Where two documents disagree the smaller limit is taken and both are named: HSE (RM 48 MHz, DS 50 MHz) and the
//! PLL2/3 VCO (RM 836 MHz, DS 960 MHz by characterization).

use exact_units::Exact;
use exact_units::prefix::{Mega, Milli};
use exact_units::units::{Hertz, Volt};

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

/// A per-VOS maximum with its source.
#[derive(Copy, Clone, Debug)]
pub struct PerVos {
    pub max: [Exact<Hertz>; 4],
    pub source: Source,
}

// --- Oscillators

pub const HSE: Range = Range {
    min: mhz(4),
    max: mhz(48),
    // RM §8.5 p.318 says 4 to 48 MHz; DS13311 Tables 35 (bypass, p.145) and 37 (crystal, p.147) say 50 MHz
    source: Source::Rm0468 {
        section: "8.5",
        page: 318,
    },
};

pub const HSI: Exact<Hertz> = mhz(64);
pub const HSI_SRC: Source = Source::Rm0468 {
    section: "8.7.2",
    page: 365,
};
/// CSI nominal (DS13311 Table 41, p.151: 3.96 to 4.04 MHz at 30 °C, more over temperature).
pub const CSI: Exact<Hertz> = mhz(4);
/// HSI48 nominal (DS13311 Table 39, p.149: −4.5 to +3.5 % over temperature without the CRS).
pub const HSI48: Exact<Hertz> = mhz(48);

// --- PLL

/// refx_ck after DIVMx (RM §8.5.5 p.327).
pub const PLL_REF: Range = Range {
    min: mhz(1),
    max: mhz(16),
    source: Source::Rm0468 {
        section: "8.5.5",
        page: 327,
    },
};

/// PLLxRGE choices (RM §8.7.11 p.381): 00 1-2 MHz, 01 2-4, 10 4-8, 11 8-16.
pub const PLL_RGE_SRC: Source = Source::Rm0468 {
    section: "8.7.11",
    page: 381,
};

/// Wide VCO, PLLxVCOSEL = 0 (RM §8.7.11 p.381; PLL2/3 DS13311 Table 45 says 960 MHz by characterization).
pub const VCO_WIDE: Range = Range {
    min: mhz(192),
    max: mhz(836),
    source: Source::Rm0468 {
        section: "8.7.11",
        page: 381,
    },
};

/// Medium VCO, PLLxVCOSEL = 1 (RM §8.7.11 p.381).
pub const VCO_MEDIUM: Range = Range {
    min: mhz(150),
    max: mhz(420),
    source: Source::Rm0468 {
        section: "8.7.11",
        page: 381,
    },
};

/// "≤ 2 MHz VCOL must be selected ... ≥ 2 MHz VCOH" (RM §8.5.5 p.327).
pub const VCOSEL_SRC: Source = Source::Rm0468 {
    section: "8.5.5",
    page: 327,
};

/// DIVMx ÷1..÷63: the range is the type [Divm](super::config::Divm).
pub const DIVM_SRC: Source = Source::Rm0468 {
    section: "8.7.10",
    page: 377,
};
/// DIVNx ×4..×512: [Divn](super::config::Divn).
pub const DIVN_SRC: Source = Source::Rm0468 {
    section: "8.7.12",
    page: 383,
};
/// DIVP/Q/Rx ÷1..÷128: [Divpqr](super::config::Divpqr).
pub const DIVPQR_SRC: Source = Source::Rm0468 {
    section: "8.7.12",
    page: 382,
};
/// PLL1 DIVP: 1 or even ratios only (RM §8.7.12 p.383, Figure 47 p.327).
pub const PLL1_DIVP_EVEN_SRC: Source = Source::Rm0468 {
    section: "8.7.12",
    page: 383,
};
/// FRACN is 13 bits (RM §8.7.13 p.384); SH_REG = 0 is integer mode (RM §8.5.5 p.328).
pub const FRACN_MAX: u16 = 8191;
pub const FRACN_SRC: Source = Source::Rm0468 {
    section: "8.5.5",
    page: 328,
};
/// "Sigma-delta mode: forbidden" with the medium VCO (DS13311 Table 44 p.153, Table 46 p.155).
pub const FRACN_MEDIUM_SRC: Source = Source::Ds13311 {
    table: 44,
    page: 153,
};
/// One PLLSRC for all three PLLs (RM §8.7.10 p.378).
pub const PLLSRC_SRC: Source = Source::Rm0468 {
    section: "8.7.10",
    page: 378,
};

/// PLL outputs, wide VCO (DS13311 Table 43 p.152 for PLL1 P, Table 45 p.154 for PLL2/3; min 1.5 MHz).
pub const PLL_OUT_WIDE: PerVos = PerVos {
    max: [mhz(550), mhz(400), mhz(300), mhz(170)],
    source: Source::Ds13311 {
        table: 45,
        page: 154,
    },
};
pub const PLL_OUT_WIDE_MIN: Exact<Hertz> = Exact::new_in::<Mega>(15, 10);
/// PLL outputs, medium VCO (DS13311 Table 44 p.153, Table 46 p.155; min 1.17 MHz).
pub const PLL_OUT_MEDIUM: PerVos = PerVos {
    max: [mhz(210), mhz(210), mhz(210), mhz(200)],
    source: Source::Ds13311 {
        table: 46,
        page: 155,
    },
};
pub const PLL_OUT_MEDIUM_MIN: Exact<Hertz> = Exact::new_in::<Mega>(117, 100);

// --- Buses (DS13311 Table 13, p.118)

const TABLE13: Source = Source::Ds13311 {
    table: 13,
    page: 118,
};
pub const CPU: PerVos = PerVos {
    max: [mhz(520), mhz(400), mhz(300), mhz(170)],
    source: TABLE13,
};
/// VOS0 with CPUFREQ_BOOST (DS13311 Table 13; RM §4.9.25 p.218: TCM ECC off).
pub const CPU_BOOST: Exact<Hertz> = mhz(550);
pub const AXI_AHB: PerVos = PerVos {
    max: [mhz(275), mhz(200), mhz(150), mhz(85)],
    source: TABLE13,
};
pub const APB: PerVos = PerVos {
    max: [
        Exact::new_in::<Mega>(1375, 10),
        mhz(100),
        mhz(75),
        Exact::new_in::<Mega>(425, 10),
    ],
    source: TABLE13,
};
/// D1CPRE and HPRE have no ÷32 (RM §8.7.7 p.373-374).
pub const HPRE_SRC: Source = Source::Rm0468 {
    section: "8.7.7",
    page: 373,
};

/// VOS0 junction temperature limit 105 °C (DS13311 Table 14, p.119).
pub const VOS0_TJ_SRC: Source = Source::Ds13311 {
    table: 14,
    page: 119,
};
/// VOS0 minimum VDD per supply (DS13311 Table 14, p.119): SMPS 2.2 V, LDO 1.7 V, SMPS supplies LDO 3.0 V, bypass
/// 1.62 V.
pub const VOS0_VDD_MIN: [(&str, Exact<Volt>); 4] = [
    ("smps", Exact::new_in::<Milli>(2200, 1)),
    ("ldo", Exact::new_in::<Milli>(1700, 1)),
    ("smps-ldo", Exact::new_in::<Milli>(3000, 1)),
    ("bypass", Exact::new_in::<Milli>(1620, 1)),
];

// --- Flash (RM §4.3.8 Table 16, p.161): the top of each wait-state band per VOS, 0 WS first.

pub const FLASH_WS: [[Option<Exact<Hertz>>; 4]; 4] = [
    // VOS0
    [
        Some(mhz(70)),
        Some(mhz(140)),
        Some(mhz(210)),
        Some(mhz(275)),
    ],
    // VOS1
    [Some(mhz(67)), Some(mhz(133)), Some(mhz(200)), None],
    // VOS2
    [Some(mhz(50)), Some(mhz(100)), Some(mhz(150)), None],
    // VOS3
    [Some(mhz(35)), Some(mhz(70)), Some(mhz(85)), None],
];
pub const FLASH_WS_SRC: Source = Source::Rm0468Table {
    table: 16,
    page: 161,
};

// --- Kernel clocks (RM Table 56, p.336-337)

const TABLE56: Source = Source::Rm0468Table {
    table: 56,
    page: 336,
};

pub const fn kernel_max(p: super::config::Periph) -> PerVos {
    use super::config::Periph::*;
    let max = match p {
        Fmc => [mhz(275), mhz(200), mhz(150), mhz(100)],
        Octospi => [mhz(333), mhz(200), mhz(150), mhz(100)],
        Sdmmc => [mhz(250), mhz(200), mhz(150), mhz(100)],
        Fdcan => [mhz(125), mhz(100), mhz(75), mhz(50)],
        I2c1235 | I2c4 | Usart16910 | Usart234578 | Lpuart1 => {
            [Exact::new_in::<Mega>(1375, 10), mhz(100), mhz(75), mhz(43)]
        }
        Spi123 | Spi45 | Spi6 => [mhz(250), mhz(180), mhz(125), mhz(75)],
        UsbOtgHs => [mhz(125), mhz(100), mhz(50), mhz(50)],
        Adc => [mhz(160), mhz(160), mhz(60), mhz(40)],
    };
    PerVos {
        max,
        source: TABLE56,
    }
}

/// Timer kernel clocks TIM1..8, 12..17 (RM Table 56, p.336).
pub const TIMERS: PerVos = PerVos {
    max: [mhz(275), mhz(200), mhz(150), mhz(85)],
    source: TABLE56,
};

/// SDMMC and ADC want a 50 % duty kernel clock: an even PLL ratio (RM Table 56 footnote 4, p.337).
pub const DUTY50_SRC: Source = Source::Rm0468Table {
    table: 56,
    page: 337,
};

/// USB OTG: "48 MHz clock from the RCC" and AHB above 30 MHz (RM §62.4.3 p.2638-2639).
pub const USB_SRC: Source = Source::Rm0468 {
    section: "62.4.3",
    page: 2638,
};
pub const USB_AHB_MIN: Exact<Hertz> = mhz(30);

/// Kernel mux choices (RM §8.7.18-8.7.21, p.391-399).
pub const MUX_SRC: Source = Source::Rm0468 {
    section: "8.7.18",
    page: 391,
};
