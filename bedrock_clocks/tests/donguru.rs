//! Donguru (typec_dongle fw/donguru on B153A, STM32G0B1CET6): the tree its embassy config sets up today, planned
//! exactly, and the checks around it.
//!
//! `BLESS=1 cargo test -p bedrock_clocks` rewrites the golden `clocks.rs`.

use bedrock_clocks::g0::emit::{Origin, emit};
use bedrock_clocks::g0::*;
use bedrock_clocks::{Severity, source_hash};
use exact_units::prefix::Mega;
use exact_units::units::Hertz;
use exact_units::{Exact, Tolerance, q};

fn mhz(n: i128) -> Exact<Hertz> {
    Exact::new_in::<Mega>(n, 1)
}

/// fw/donguru/src/main.rs `boot`: HSI16 ÷ 1 × 8 = 128 MHz VCO, R ÷ 2 = 64 MHz SYSCLK, P ÷ 4 = 32 MHz for the ADC,
/// AHB and APB undivided, USB on HSI48 trimmed by the CRS from the SOF, UCPD on HSI16, I2C2 on PCLK.
pub fn donguru() -> Config {
    Config {
        part: Part::parse("STM32G0B1CET6").unwrap(),
        vos: Vos::Range1,
        dual_bank_option: false,
        // off: the image spans both banks (ES0548 2.2.10, typec_dongle FW-38)
        prefetch: false,
        icache: true,
        hse: None,
        hsi16: Some(HsiDiv::DIV1),
        hsi48: Some(Hsi48 {
            crs: Some(CrsSync::UsbSof),
        }),
        lse: None,
        pll: Some(Pll::from_ratios(PllSource::Hsi16, 1, 8, Some(4), None, Some(2)).unwrap()),
        sys: Sys {
            source: SysSource::PllR,
            hpre: Hpre::DIV1,
            ppre: Ppre::DIV1,
        },
        kernel: Kernel {
            adc: Ker::PllP,
            usb: Ker::Hsi48,
            ..Kernel::default()
        },
        used: vec![Periph::Adc, Periph::Usb, Periph::Ucpd, Periph::I2c2],
        expect: vec![
            Expect {
                periph: Periph::Usb,
                freq: mhz(48),
                tolerance: Tolerance::EXACT,
            },
            Expect {
                periph: Periph::Adc,
                freq: mhz(32),
                tolerance: q!(+- 1 %).into(),
            },
        ],
        allow: Allow::default(),
    }
}

#[test]
fn donguru_tree_is_clean_and_exact() {
    let c = donguru();
    let p = plan(&c).unwrap_or_else(|f| panic!("{f:#?}"));
    assert!(p.warnings.is_empty(), "{:#?}", p.warnings);
    let t = &p.tree;
    assert!(t.sysclk.unwrap() == q!(64 MHz));
    assert!(t.hclk.unwrap() == q!(64 MHz));
    assert!(t.pclk.unwrap() == q!(64 MHz));
    assert!(t.pll.unwrap().vco == q!(128 MHz));
    assert!(t.kernel[&Periph::Adc].unwrap() == q!(32 MHz));
    assert!(t.kernel[&Periph::Usb].unwrap() == q!(48 MHz));
    assert!(t.kernel[&Periph::Ucpd].unwrap() == q!(16 MHz));
    // 64 MHz in Range 1 is 2 wait states (RM0444 Table 12); ICEN on, PRFTEN off
    assert_eq!(p.regs.flash_acr, 0x202);
    // PLLSRC HSI16 (10), PLLM 0 (÷1), PLLN 8, PLLP 3 (÷4) with PLLPEN, PLLQ 1 (÷2, off), PLLR 1 (÷2) with PLLREN
    assert_eq!(
        p.regs.pllcfgr,
        0b10 | (8 << 8) | (1 << 16) | (3 << 17) | (1 << 25) | (1 << 28) | (1 << 29)
    );
    // SW PLLRCLK, HPRE and PPRE ÷1
    assert_eq!(p.regs.cfgr, 2);
    // ADCSEL PLLPCLK; USBSEL HSI48 is 0
    assert_eq!(p.regs.ccipr, 1 << 30);
    assert_eq!(p.regs.ccipr2, 0);
    // the CRS: RELOAD 47999 and FELIM 34, the reset value's (RM0444 §6.7.2: 0x2022_BB7F)
    assert_eq!(p.regs.crs_cfgr, 0x2022_BB7F);
    assert_eq!(p.regs.crs_cr, 0x4060);
    assert_eq!(p.regs.pwr_cr1_vos, 1 << 9);
}

#[test]
fn golden_clocks_rs() {
    let c = donguru();
    let p = plan(&c).unwrap_or_else(|f| panic!("{f:#?}"));
    let toml = include_bytes!("golden/donguru_clocks.toml");
    let out = emit(
        &c,
        &p.tree,
        &p.regs,
        &Origin {
            path: "fw/donguru/clocks.toml",
            hash: source_hash(toml),
        },
    );
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/golden/donguru_clocks.rs"
    );
    if std::env::var_os("BLESS").is_some() {
        std::fs::write(path, &out).unwrap();
    }
    let golden = std::fs::read_to_string(path).unwrap_or_default();
    assert_eq!(
        out, golden,
        "run BLESS=1 cargo test -p bedrock_clocks to update"
    );
    if let Ok(o) = std::process::Command::new("rustfmt")
        .args(["--edition", "2024", "--check", path])
        .output()
    {
        assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stdout));
    }
}

fn errors(c: &Config) -> Vec<String> {
    check(c)
        .into_iter()
        .filter(|f| f.severity == Severity::Error)
        .map(|f| f.to_string())
        .collect()
}

#[test]
fn findings_name_key_value_and_source() {
    // N × 22: a 352 MHz VCO
    let mut c = donguru();
    c.pll.as_mut().unwrap().n = plln(22).unwrap();
    let e = errors(&c);
    assert!(
        e.contains(
            &"error: pll.n: PLL VCO 352 MHz outside 96 MHz..=344 MHz, the Range1 limits [DS13560 Rev 6 Table 47 p.88]"
                .to_string()
        ),
        "{e:#?}"
    );

    // SYSCLK above 64 MHz from R ÷ 2 of a 160 MHz VCO, and with it HCLK and the flash table
    let mut c = donguru();
    c.pll.as_mut().unwrap().n = plln(10).unwrap();
    let e = errors(&c);
    assert!(
        e.iter()
            .any(|f| f.starts_with("error: pll.r: PLLRCLK 80 MHz")),
        "{e:#?}"
    );
    assert!(
        e.iter()
            .any(|f| f.starts_with("error: sys.source: SYSCLK 80 MHz")),
        "{e:#?}"
    );
    assert!(
        e.iter().any(|f| f.contains("no flash wait-state setting")),
        "{e:#?}"
    );

    // Range 2: 16 MHz SYSCLK, HSI48 refused
    let mut c = donguru();
    c.vos = Vos::Range2;
    let e = errors(&c);
    assert!(
        e.iter()
            .any(|f| f.starts_with("error: hsi48: HSI48 isn't available in Range 2")),
        "{e:#?}"
    );
    assert!(
        e.iter()
            .any(|f| f.starts_with("error: sys.source: SYSCLK 64 MHz outside 0 Hz..=16 MHz")),
        "{e:#?}"
    );

    // prefetch on a 512 KiB part: ES0548 2.2.10, unless allowed by name
    let mut c = donguru();
    c.prefetch = true;
    assert!(
        errors(&c)
            .iter()
            .any(|f| f.contains("[ES0548 Rev 4 erratum 2.2.10]"))
    );
    c.allow.prefetch_dual_bank = true;
    assert!(errors(&c).is_empty());
    // a 256 KiB part in single-bank mode may prefetch
    let mut c = donguru();
    c.part = Part::parse("STM32G0B1CCT6").unwrap();
    c.prefetch = true;
    assert!(errors(&c).is_empty());

    // USB on HSI48 without the CRS, and UCPD without HSI16
    let mut c = donguru();
    c.hsi48 = Some(Hsi48 { crs: None });
    assert!(errors(&c).iter().any(|f| f.starts_with("error: hsi48.crs")));
    let mut c = donguru();
    c.hsi16 = None;
    let e = errors(&c);
    assert!(
        e.iter()
            .any(|f| f.starts_with("error: kernel.ucpd: UCPD is in use")),
        "{e:#?}"
    );
    assert!(
        e.iter().any(|f| f.starts_with("error: pll.source")),
        "{e:#?}"
    );

    // a mux choice the register doesn't have
    let mut c = donguru();
    c.kernel.usb = Ker::Sysclk;
    assert!(
        errors(&c)
            .iter()
            .any(|f| f.starts_with("error: kernel.usb: this mux can't select Sysclk"))
    );

    // the ADC target named with its exact error
    let mut c = donguru();
    c.pll.as_mut().unwrap().p = Some(pllp(5).unwrap());
    let e = errors(&c);
    assert!(
        e.contains(&"error: expect.adc: 25.6 MHz is -20 % off 32 MHz, outside ±1 %".to_string()),
        "{e:#?}"
    );
}

#[test]
fn warnings_errata_and_fadc() {
    // HSIDIV ÷2: ES0548 2.2.4
    let mut c = donguru();
    c.hsi16 = Some(HsiDiv::from_ratio(2).unwrap());
    assert!(
        check(&c)
            .iter()
            .any(|f| f.severity == Severity::Warning && f.key == "hsi16.div")
    );
    // the ADC kernel clock above fADC 35 MHz wants ADC_CCR PRESC
    let mut c = donguru();
    c.pll.as_mut().unwrap().p = Some(pllp(2).unwrap());
    c.expect.pop();
    let f = check(&c);
    assert!(
        f.iter().any(|f| f.severity == Severity::Warning
            && f.to_string()
                == "warning: kernel.adc: ADC kernel clock 64 MHz above fADC 35 MHz: ADC_CCR PRESC must divide it [DS13560 Rev 6 Table 63 p.100]"),
        "{f:#?}"
    );
}

#[test]
fn knobs_refuse_what_the_register_cannot_hold() {
    assert_eq!(
        pllm(9).unwrap_err().to_string(),
        "m: PLLM ÷9 is out of 1..=8 (Ratio, u8) [RM0444 Rev 5 §5.4.4 p.185]"
    );
    assert!(plln(87).is_err() && plln(7).is_err() && plln(86).is_ok());
    assert!(pllp(1).is_err() && pllp(32).is_ok());
    assert!(pllqr("q", 9).is_err());
    assert!(Hpre::from_ratio(32).is_err());
    assert!(Ppre::from_ratio(32).is_err());
    assert!(HsiDiv::from_ratio(3).is_err());
    // the HPRE / PPRE field values (RM0444 §5.4.3)
    for (r, bits) in [(1, 0), (2, 8), (16, 11), (64, 12), (512, 15)] {
        assert_eq!(Hpre::from_ratio(r).unwrap().bits(), bits);
    }
    for (r, bits) in [(1, 0), (2, 4), (16, 7)] {
        assert_eq!(Ppre::from_ratio(r).unwrap().bits(), bits);
    }
}

/// HSE through the PLL, every mux at a non-zero choice once, and the encoding of each.
#[test]
fn hse_and_every_mux() {
    let mut c = donguru();
    c.hse = Some(Hse {
        freq: mhz(8),
        mode: HseMode::Bypass,
    });
    c.lse = Some(Exact::new(32_768, 1));
    c.pll = Some(Pll::from_ratios(PllSource::Hse, 1, 16, Some(4), Some(4), Some(2)).unwrap());
    c.kernel = Kernel {
        adc: Ker::Hsi16,
        rng: Ker::PllQ,
        rng_div: 8,
        tim15: Ker::PllQ,
        tim1: Ker::PllQ,
        lptim2: Ker::Lse,
        lptim1: Ker::Lsi,
        i2c2: Ker::Sysclk,
        i2c1: Ker::Hsi16,
        lpuart1: Ker::Lse,
        lpuart2: Ker::Hsi16,
        usart3: Ker::Sysclk,
        usart2: Ker::Lse,
        usart1: Ker::Hsi16,
        usb: Ker::PllQ,
        fdcan: Ker::Hse,
        i2s2: Ker::Hsi16,
        i2s1: Ker::PllP,
    };
    c.hsi48 = None;
    c.expect.clear();
    c.used = vec![Periph::Usb, Periph::Fdcan];
    // USB at 128 MHz / 4 = 32 MHz
    let e = errors(&c);
    assert_eq!(
        e,
        vec![
            "error: kernel.usb: USB needs exactly 48 MHz, got 32 MHz [RM0444 Rev 5 §37.5.2 p.1268]"
        ]
    );
    c.pll = Some(Pll::from_ratios(PllSource::Hse, 1, 24, Some(4), Some(4), Some(3)).unwrap());
    let p = plan(&c).unwrap_or_else(|f| panic!("{f:#?}"));
    assert_eq!(p.tree.kernel[&Periph::Usb], Some(mhz(48)));
    assert_eq!(p.regs.rcc_cr_osc, (1 << 8) | (1 << 16) | (1 << 18));
    assert_eq!(p.regs.crs_cr, 0);
    assert_eq!(
        p.regs.ccipr,
        0b10 | (0b11 << 2)
            | (0b01 << 4)
            | (0b10 << 8)
            | (0b11 << 10)
            | (0b10 << 12)
            | (0b01 << 14)
            | (0b01 << 18)
            | (0b11 << 20)
            | (1 << 22)
            | (1 << 24)
            | (0b11 << 26)
            | (0b11 << 28)
            | (0b10 << 30)
    );
    assert_eq!(
        p.regs.ccipr2,
        0b01 | (0b10 << 2) | (0b10 << 8) | (0b01 << 12)
    );
}
