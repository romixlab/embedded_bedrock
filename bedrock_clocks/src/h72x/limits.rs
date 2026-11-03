//! The RM0468 family's limits, each with the document location it was read from. Per-VOS tables are indexed by
//! [Vos::idx](super::config::Vos::idx): VOS0, VOS1, VOS2, VOS3.
//!
//! Where two documents disagree the smaller limit is taken and both are named: HSE (RM 48 MHz, DS 50 MHz) and the
//! PLL2/3 VCO (RM 836 MHz, DS 960 MHz by characterization).

use crate::ratio::Hz;
use crate::source::Source;

/// A closed range of frequencies with its source.
#[derive(Copy, Clone, Debug)]
pub struct Range {
    pub min: Hz,
    pub max: Hz,
    pub source: Source,
}

/// A per-VOS maximum with its source.
#[derive(Copy, Clone, Debug)]
pub struct PerVos {
    pub max: [Hz; 4],
    pub source: Source,
}

// --- Oscillators

pub const HSE: Range = Range {
    min: Hz::mhz(4),
    max: Hz::mhz(48),
    // RM §8.5 p.318 says 4 to 48 MHz; DS13311 Tables 35 (bypass, p.145) and 37 (crystal, p.147) say 50 MHz
    source: Source::Rm0468 {
        section: "8.5",
        page: 318,
    },
};

pub const HSI: Hz = Hz::mhz(64);
pub const HSI_SRC: Source = Source::Rm0468 {
    section: "8.7.2",
    page: 365,
};
/// CSI nominal (DS13311 Table 41, p.151: 3.96 to 4.04 MHz at 30 °C, more over temperature).
pub const CSI: Hz = Hz::mhz(4);
/// HSI48 nominal (DS13311 Table 39, p.149: −4.5 to +3.5 % over temperature without the CRS).
pub const HSI48: Hz = Hz::mhz(48);

// --- PLL

/// refx_ck after DIVMx (RM §8.5.5 p.327).
pub const PLL_REF: Range = Range {
    min: Hz::mhz(1),
    max: Hz::mhz(16),
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
    min: Hz::mhz(192),
    max: Hz::mhz(836),
    source: Source::Rm0468 {
        section: "8.7.11",
        page: 381,
    },
};

/// Medium VCO, PLLxVCOSEL = 1 (RM §8.7.11 p.381).
pub const VCO_MEDIUM: Range = Range {
    min: Hz::mhz(150),
    max: Hz::mhz(420),
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

pub const DIVM: (u32, u32, Source) = (
    1,
    63,
    Source::Rm0468 {
        section: "8.7.10",
        page: 377,
    },
);
pub const DIVN: (u32, u32, Source) = (
    4,
    512,
    Source::Rm0468 {
        section: "8.7.12",
        page: 383,
    },
);
pub const DIVPQR: (u32, u32, Source) = (
    1,
    128,
    Source::Rm0468 {
        section: "8.7.12",
        page: 382,
    },
);
/// PLL1 DIVP: 1 or even ratios only (RM §8.7.12 p.383, Figure 47 p.327).
pub const PLL1_DIVP_EVEN_SRC: Source = Source::Rm0468 {
    section: "8.7.12",
    page: 383,
};
/// FRACN is 13 bits (RM §8.7.13 p.384); SH_REG = 0 is integer mode (RM §8.5.5 p.328).
pub const FRACN_MAX: u32 = 8191;
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
    max: [Hz::mhz(550), Hz::mhz(400), Hz::mhz(300), Hz::mhz(170)],
    source: Source::Ds13311 {
        table: 45,
        page: 154,
    },
};
pub const PLL_OUT_WIDE_MIN: Hz = Hz::mhz_tenths(15);
/// PLL outputs, medium VCO (DS13311 Table 44 p.153, Table 46 p.155; min 1.17 MHz).
pub const PLL_OUT_MEDIUM: PerVos = PerVos {
    max: [Hz::mhz(210), Hz::mhz(210), Hz::mhz(210), Hz::mhz(200)],
    source: Source::Ds13311 {
        table: 46,
        page: 155,
    },
};
pub const PLL_OUT_MEDIUM_MIN: Hz = Hz::mhz_hundredths(117);

// --- Buses (DS13311 Table 13, p.118)

const TABLE13: Source = Source::Ds13311 {
    table: 13,
    page: 118,
};
pub const CPU: PerVos = PerVos {
    max: [Hz::mhz(520), Hz::mhz(400), Hz::mhz(300), Hz::mhz(170)],
    source: TABLE13,
};
/// VOS0 with CPUFREQ_BOOST (DS13311 Table 13; RM §4.9.25 p.218: TCM ECC off).
pub const CPU_BOOST: Hz = Hz::mhz(550);
pub const AXI_AHB: PerVos = PerVos {
    max: [Hz::mhz(275), Hz::mhz(200), Hz::mhz(150), Hz::mhz(85)],
    source: TABLE13,
};
pub const APB: PerVos = PerVos {
    max: [
        Hz::mhz_tenths(1375),
        Hz::mhz(100),
        Hz::mhz(75),
        Hz::mhz_tenths(425),
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
pub const VOS0_VDD_MIN_MV: [(&str, u32); 4] = [
    ("smps", 2200),
    ("ldo", 1700),
    ("smps-ldo", 3000),
    ("bypass", 1620),
];

// --- Flash (RM §4.3.8 Table 16, p.161): the top of each wait-state band per VOS, 0 WS first.

pub const FLASH_WS: [[Option<Hz>; 4]; 4] = [
    // VOS0
    [
        Some(Hz::mhz(70)),
        Some(Hz::mhz(140)),
        Some(Hz::mhz(210)),
        Some(Hz::mhz(275)),
    ],
    // VOS1
    [
        Some(Hz::mhz(67)),
        Some(Hz::mhz(133)),
        Some(Hz::mhz(200)),
        None,
    ],
    // VOS2
    [
        Some(Hz::mhz(50)),
        Some(Hz::mhz(100)),
        Some(Hz::mhz(150)),
        None,
    ],
    // VOS3
    [
        Some(Hz::mhz(35)),
        Some(Hz::mhz(70)),
        Some(Hz::mhz(85)),
        None,
    ],
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
        Fmc => [Hz::mhz(275), Hz::mhz(200), Hz::mhz(150), Hz::mhz(100)],
        Octospi => [Hz::mhz(333), Hz::mhz(200), Hz::mhz(150), Hz::mhz(100)],
        Sdmmc => [Hz::mhz(250), Hz::mhz(200), Hz::mhz(150), Hz::mhz(100)],
        Fdcan => [Hz::mhz(125), Hz::mhz(100), Hz::mhz(75), Hz::mhz(50)],
        I2c1235 | I2c4 | Usart16910 | Usart234578 | Lpuart1 => {
            [Hz::mhz_tenths(1375), Hz::mhz(100), Hz::mhz(75), Hz::mhz(43)]
        }
        Spi123 | Spi45 | Spi6 => [Hz::mhz(250), Hz::mhz(180), Hz::mhz(125), Hz::mhz(75)],
        UsbOtgHs => [Hz::mhz(125), Hz::mhz(100), Hz::mhz(50), Hz::mhz(50)],
        Adc => [Hz::mhz(160), Hz::mhz(160), Hz::mhz(60), Hz::mhz(40)],
    };
    PerVos {
        max,
        source: TABLE56,
    }
}

/// Timer kernel clocks TIM1..8, 12..17 (RM Table 56, p.336).
pub const TIMERS: PerVos = PerVos {
    max: [Hz::mhz(275), Hz::mhz(200), Hz::mhz(150), Hz::mhz(85)],
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
pub const USB_AHB_MIN: Hz = Hz::mhz(30);

/// Kernel mux choices (RM §8.7.18-8.7.21, p.391-399).
pub const MUX_SRC: Source = Source::Rm0468 {
    section: "8.7.18",
    page: 391,
};
