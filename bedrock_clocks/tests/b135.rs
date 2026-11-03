//! B135 rev B (STM32H725IG, 24 MHz HSE bypass): the tree the firmware runs today, and the planned one.
//!
//! `BLESS=1 cargo test -p bedrock_clocks` rewrites the golden `clocks.rs`.

use bedrock_clocks::h72x::emit::{Origin, emit};
use bedrock_clocks::h72x::*;
use bedrock_clocks::{Hz, source_hash};

fn pll(
    m: u8,
    n: u16,
    fracn: Option<u16>,
    p: Option<u8>,
    q: Option<u8>,
    r: Option<u8>,
) -> Option<Pll> {
    Some(Pll {
        m,
        n,
        fracn,
        p,
        q,
        r,
        steer: None,
    })
}

fn base() -> Config {
    Config {
        part: Part::parse("STM32H725IGK6").unwrap(),
        vdd_mv: 3300,
        supply: Supply::DirectSmps,
        vos: Vos::Vos1,
        cpu_boost: false,
        hse: Some(Hse {
            freq: Hz::mhz(24),
            mode: HseMode::Bypass,
        }),
        hsi_div: None,
        csi: false,
        hsi48: false,
        lse: None,
        pll_source: PllSource::Hse,
        pll: [None; 3],
        sys: Sys {
            source: SysSource::Pll1P,
            d1cpre: 1,
            hpre: 2,
            d1ppre: 2,
            d2ppre1: 2,
            d2ppre2: 2,
            d3ppre: 2,
            timpre: false,
        },
        kernel: Kernel {
            fdcan: Ker::Pll2Q,
            sdmmc: Ker::Pll2R,
            usb: Ker::Pll3Q,
            adc: Ker::Pll3R,
            ..Kernel::default()
        },
        used: vec![Periph::Sdmmc, Periph::Fdcan, Periph::UsbOtgHs, Periph::Adc],
        expect: vec![
            Expect {
                periph: Periph::UsbOtgHs,
                freq: Hz::mhz(48),
                tolerance_ppm: 0,
            },
            Expect {
                periph: Periph::Fdcan,
                freq: Hz::mhz(80),
                tolerance_ppm: 1,
            },
        ],
        allow: Allow::default(),
    }
}

/// What vb135_fdcan_iso_usb_fw 0.2.6 runs (b135_fw/src/clocks.rs): VOS0, PLL1 at 519.999755859375 MHz.
fn today() -> Config {
    let mut c = base();
    c.vos = Vos::Vos0;
    c.pll = [
        pll(8, 173, Some(2730), Some(1), Some(4), Some(2)),
        pll(8, 133, Some(2730), None, Some(5), Some(2)),
        pll(2, 16, None, Some(2), Some(4), Some(4)),
    ];
    c.pll[0].as_mut().unwrap().steer = Some(SteerPpm {
        min: -100,
        max: 100,
    });
    c
}

/// The plan for B135 rev B: VOS1 for lifetime (AN5337 Rev 4 §7), a round 398 MHz CPU (8 MHz × (99 + 4096/8192)
/// ÷ 2) with FRACN mid-range, so ±100 ppm of steering on PLL1 and PLL2 keeps every VOS1 limit at both ends.
pub fn planned() -> Config {
    let mut c = base();
    c.pll = [
        pll(3, 99, Some(4096), Some(2), Some(8), None),
        pll(5, 166, Some(5461), None, Some(10), Some(8)),
        pll(3, 96, None, Some(8), Some(16), Some(16)),
    ];
    for i in 0..2 {
        c.pll[i].as_mut().unwrap().steer = Some(SteerPpm {
            min: -100,
            max: 100,
        });
    }
    c
}

#[test]
fn todays_tree_is_refused_at_the_upper_steer_end() {
    let f = check(&today());
    let text: Vec<String> = f.iter().map(|f| f.to_string()).collect();
    let all = text.join("\n");
    // the +100 ppm end of PLL1 takes the CPU over 520 MHz
    assert!(
        f.iter().any(|f| f.severity == Severity::Error
            && f.key == "sys.d1cpre"
            && f.message.contains("+100 ppm")),
        "{all}"
    );
    // VOS0 is flagged for lifetime
    assert!(
        f.iter()
            .any(|f| f.key == "vos" && f.message.contains("2 years")),
        "{all}"
    );
}

#[test]
fn planned_tree_is_clean_and_exact() {
    let c = planned();
    let p = plan(&c).unwrap_or_else(|f| panic!("{f:#?}"));
    assert!(p.warnings.is_empty(), "{:#?}", p.warnings);
    let t = &p.tree;
    assert_eq!(t.sys_ck, Some(Hz::mhz(398)));
    assert_eq!(t.kernel[&Periph::UsbOtgHs], Some(Hz::mhz(48)));
    assert_eq!(
        t.kernel[&Periph::Fdcan].unwrap().to_string(),
        "79.99998046875 MHz"
    );
    // flash: AXI 199 MHz at VOS1 is 2 wait states (RM0468 Table 16)
    assert_eq!(p.regs.flash_acr, 0x22);
    // PLL1 DIVR: DIVN 98, DIVP 1 (÷2), DIVQ 7 (÷8), DIVR 0 (off)
    assert_eq!(p.regs.plldivr[0], 98 | (1 << 9) | (7 << 16));
    assert_eq!(p.regs.pllfracr[0], 4096 << 3);
    // VOS1 is 0b11 in PWR_D3CR
    assert_eq!(p.regs.pwr_d3cr_vos, 0b11 << 14);
    // direct SMPS: SDEN only, bit 6 at its reset value
    assert_eq!(p.regs.pwr_cr3, 0x44);
}

#[test]
fn golden_clocks_rs() {
    let c = planned();
    let p = plan(&c).unwrap_or_else(|f| panic!("{f:#?}"));
    let toml = include_bytes!("golden/b135b_clocks.toml");
    let out = emit(
        &c,
        &p.tree,
        &p.regs,
        &Origin {
            path: "boards/b135b/clocks.toml",
            hash: source_hash(toml),
        },
    );
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/golden/b135b_clocks.rs");
    if std::env::var_os("BLESS").is_some() {
        std::fs::write(path, &out).unwrap();
    }
    let golden = std::fs::read_to_string(path).unwrap_or_default();
    assert_eq!(
        out, golden,
        "run BLESS=1 cargo test -p bedrock_clocks to update"
    );
    // the generated file is rustfmt-clean, so a regenerated file shows no format churn in review
    if let Ok(o) = std::process::Command::new("rustfmt")
        .args(["--edition", "2024", "--check", path])
        .output()
    {
        assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stdout));
    }
}

#[test]
fn findings_name_key_value_and_source() {
    let mut c = planned();
    c.pll[0].as_mut().unwrap().n = 110; // VCO 8 MHz × 110.5 = 884 MHz
    let f = check(&c);
    let vco = f.iter().find(|f| f.key == "pll1.n").expect("a VCO finding");
    assert_eq!(
        vco.to_string(),
        "error: pll1.n: PLL1 VCO (wide) 884 MHz outside 192 MHz..=836 MHz [RM0468 Rev 3 §8.7.11 p.381]"
    );
    let mut c = planned();
    c.cpu_boost = true;
    assert!(
        check(&c)
            .iter()
            .any(|f| f.key == "cpu_boost" && f.severity == Severity::Error)
    );
    let mut c = planned();
    c.pll[0].as_mut().unwrap().p = Some(3);
    assert!(
        check(&c)
            .iter()
            .any(|f| f.key == "pll1.p" && f.message.contains("only ÷1 or even"))
    );
    let mut c = planned();
    c.part = Part::parse("STM32H725RGV6").unwrap();
    c.supply = Supply::Ldo;
    assert!(check(&c).iter().any(|f| f.key == "supply"));
}

#[test]
fn steer_end_outside_fracn_is_refused() {
    let mut c = planned();
    c.pll[0].as_mut().unwrap().fracn = Some(50); // -100 ppm is ~82 steps below
    let f = check(&c);
    assert!(
        f.iter()
            .any(|f| f.key == "pll1.steer" && f.message.contains("FRACN 0 is integer mode")),
        "{f:#?}"
    );
}

/// The encoder's bit positions against stm32-metapac's register types, field by field, for the planned tree.
#[test]
fn field_positions_match_the_pac() {
    use stm32_metapac::flash::regs as fr;
    use stm32_metapac::pwr::{regs as pr, vals as pv};
    use stm32_metapac::rcc::{regs as rr, vals as rv};

    let c = planned();
    let p = plan(&c).unwrap();
    let r = p.regs;

    let mut cr3 = pr::Cr3(0x40);
    cr3.set_sden(true);
    assert_eq!(cr3.0, r.pwr_cr3);
    let mut cr3 = pr::Cr3(0x40);
    cr3.set_sden(true);
    cr3.set_ldoen(true);
    cr3.set_sdexthp(true);
    cr3.set_sdlevel(pv::Sdlevel::from_bits(0b10));
    assert_eq!(
        cr3.0,
        bedrock_clocks::h72x::regs::pwr_cr3(Supply::SmpsExternalLdo(SmpsLevel::V2_5))
    );
    let mut cr3 = pr::Cr3(0x40);
    cr3.set_bypass(true);
    assert_eq!(cr3.0, bedrock_clocks::h72x::regs::pwr_cr3(Supply::Bypass));

    for (v, bits) in [
        (Vos::Vos0, 0),
        (Vos::Vos1, 3),
        (Vos::Vos2, 2),
        (Vos::Vos3, 1),
    ] {
        let mut d3 = pr::D3cr(0);
        d3.set_vos(pv::Vos::from_bits(bits));
        assert_eq!(d3.0, bedrock_clocks::h72x::regs::vos_bits(v));
    }

    let mut acr = fr::Acr(0);
    acr.set_latency(2);
    acr.set_wrhighfreq(2);
    assert_eq!(acr.0, r.flash_acr);

    let mut cr = rr::Cr(0);
    cr.set_hseon(true);
    cr.set_hsebyp(true);
    assert_eq!(cr.0, r.rcc_cr_osc);
    let mut cr = rr::Cr(0);
    for i in 0..3 {
        cr.set_pllon(i, true);
    }
    assert_eq!(cr.0, r.rcc_cr_pll_on);
    let mut cr = rr::Cr(0);
    cr.set_hsion(true);
    cr.set_hsidiv(rv::Hsidiv::from_bits(2));
    cr.set_csion(true);
    cr.set_hsi48on(true);
    let mut c2 = planned();
    c2.hsi_div = Some(4);
    c2.csi = true;
    c2.hsi48 = true;
    c2.hse = None;
    c2.pll_source = PllSource::Hsi;
    // HSI 16 MHz ÷ 8 = 2 MHz refs; the targets and steering are about HSE, so drop them here
    c2.expect.clear();
    for p in c2.pll.iter_mut().flatten() {
        p.m = 8;
        p.steer = None;
    }
    let p3 = c2.pll[2].as_mut().unwrap();
    p3.n = 96; // 192 MHz VCO
    (p3.p, p3.q, p3.r) = (Some(2), Some(4), Some(4));
    let p2 = plan(&c2).unwrap_or_else(|f| panic!("{f:#?}"));
    assert_eq!(cr.0, p2.regs.rcc_cr_osc);

    let mut ck = rr::Pllckselr(0);
    ck.set_pllsrc(rv::Pllsrc::from_bits(2));
    ck.set_divm(0, rv::Pllm::from_bits(3));
    ck.set_divm(1, rv::Pllm::from_bits(5));
    ck.set_divm(2, rv::Pllm::from_bits(3));
    assert_eq!(ck.0, r.pllckselr);

    let mut cfg = rr::Pllcfgr(0);
    for (i, frac, rge, pen, qen, ren) in [
        (0, true, 3, true, true, false),
        (1, true, 2, false, true, true),
        (2, false, 3, true, true, true),
    ] {
        cfg.set_pllfracen(i, frac);
        cfg.set_pllvcosel(i, rv::Pllvcosel::from_bits(0));
        cfg.set_pllrge(i, rv::Pllrge::from_bits(rge));
        cfg.set_divpen(i, pen);
        cfg.set_divqen(i, qen);
        cfg.set_divren(i, ren);
    }
    assert_eq!(cfg.0, r.pllcfgr);

    let mut d = rr::Plldivr(0);
    d.set_plln(rv::Plln::from_bits(98));
    d.set_pllp(rv::Plldiv::from_bits(1));
    d.set_pllq(rv::Plldiv::from_bits(7));
    assert_eq!(d.0, r.plldivr[0]);
    let mut f = rr::Pllfracr(0);
    f.set_fracn(4096);
    assert_eq!(f.0, r.pllfracr[0]);

    let mut cfgr = rr::Cfgr(0);
    cfgr.set_sw(rv::Sw::from_bits(3));
    assert_eq!(cfgr.0, r.cfgr);
    let mut d1 = rr::D1cfgr(0);
    d1.set_d1cpre(rv::Hpre::from_bits(0));
    d1.set_hpre(rv::Hpre::from_bits(8));
    d1.set_d1ppre(rv::Ppre::from_bits(4));
    assert_eq!(d1.0, r.d1cfgr);
    let mut d2 = rr::D2cfgr(0);
    d2.set_d2ppre1(rv::Ppre::from_bits(4));
    d2.set_d2ppre2(rv::Ppre::from_bits(4));
    assert_eq!(d2.0, r.d2cfgr);
    let mut d3 = rr::D3cfgr(0);
    d3.set_d3ppre(rv::Ppre::from_bits(4));
    assert_eq!(d3.0, r.d3cfgr);

    let mut k1 = rr::D1ccipr(0);
    k1.set_sdmmcsel(rv::Sdmmcsel::from_bits(1));
    assert_eq!(k1.0, r.d1ccipr);
    let mut k2 = rr::D2ccip1r(0);
    k2.set_fdcansel(rv::Fdcansel::from_bits(2));
    assert_eq!(k2.0, r.d2ccip1r);
    let mut k3 = rr::D2ccip2r(0);
    k3.set_usbsel(rv::Usbsel::from_bits(2));
    assert_eq!(k3.0, r.d2ccip2r);
    let mut k4 = rr::D3ccipr(0);
    k4.set_adcsel(rv::Adcsel::from_bits(1));
    assert_eq!(k4.0, r.d3ccipr);

    // every other mux field once, at a non-zero value
    let mut c3 = planned();
    c3.hsi_div = Some(1);
    c3.csi = true;
    c3.kernel = Kernel {
        per_ck: Ker::Hse,
        octospi: Ker::Pll1Q,
        fmc: Ker::PerCk,
        spi45: Ker::Hse,
        spi123: Ker::PerCk,
        i2c1235: Ker::CsiKer,
        usart16910: Ker::Pll3Q,
        usart234578: Ker::CsiKer,
        spi6: Ker::HsiKer,
        i2c4: Ker::Pll3R,
        lpuart1: Ker::Pll2Q,
        ..c3.kernel
    };
    let p3 = plan(&c3).unwrap_or_else(|f| panic!("{f:#?}"));
    let mut k1 = rr::D1ccipr(0);
    k1.set_persel(rv::Persel::from_bits(2));
    k1.set_octospisel(rv::Fmcsel::from_bits(1));
    k1.set_fmcsel(rv::Fmcsel::from_bits(3));
    k1.set_sdmmcsel(rv::Sdmmcsel::from_bits(1));
    assert_eq!(k1.0, p3.regs.d1ccipr);
    let mut k2 = rr::D2ccip1r(0);
    k2.set_spi45sel(rv::Spi45sel::from_bits(5));
    k2.set_spi123sel(rv::Saisel::from_bits(4));
    k2.set_fdcansel(rv::Fdcansel::from_bits(2));
    assert_eq!(k2.0, p3.regs.d2ccip1r);
    let mut k3 = rr::D2ccip2r(0);
    k3.set_i2c1235sel(rv::I2c1235sel::from_bits(3));
    k3.set_usart16910sel(rv::Usart16910sel::from_bits(2));
    k3.set_usart234578sel(rv::Usart234578sel::from_bits(4));
    k3.set_usbsel(rv::Usbsel::from_bits(2));
    assert_eq!(k3.0, p3.regs.d2ccip2r);
    let mut k4 = rr::D3ccipr(0);
    k4.set_spi6sel(rv::Spi6sel::from_bits(3));
    k4.set_i2c4sel(rv::I2c4sel::from_bits(1));
    k4.set_lpuart1sel(rv::Lpuartsel::from_bits(1));
    k4.set_adcsel(rv::Adcsel::from_bits(1));
    assert_eq!(k4.0, p3.regs.d3ccipr);
}
