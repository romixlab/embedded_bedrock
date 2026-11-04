//! Every voltage scale, not only B135's: a valid tree at each of VOS0..VOS3, a refusal just over each limit of
//! DS13311 Table 13 / Tables 43-46 and RM0468 Table 56, the flash wait-state bands of RM0468 Table 16, VOS0 and the
//! CPU boost behind their allows, every PLL source, integer and fractional mode, wide and medium VCO.
//!
//! "Just over" is one FRACN step (8 MHz / 8192 = 976.5625 Hz of VCO) above the limit wherever the limit is a whole
//! number of steps, and 1 MHz on the medium VCO, which has no fractional mode.

use bedrock_clocks::h72x::check::flash_wait_states;
use bedrock_clocks::h72x::limits::{self, PerVos};
use bedrock_clocks::h72x::*;
use exact_units::Exact;
use exact_units::prefix::{Mega, Milli, One};
use exact_units::units::Hertz;

const ALL: [Vos; 4] = [Vos::Vos0, Vos::Vos1, Vos::Vos2, Vos::Vos3];

/// The ref_ck of every PLL here: HSE 24 MHz ÷ 3.
const REF_HZ: u128 = 8_000_000;

fn mhz(n: i128) -> Exact<Hertz> {
    Exact::new_in::<Mega>(n, 1)
}

fn hz(h: Exact<Hertz>) -> u128 {
    h.to::<One, u64>().expect("whole Hz").get().into()
}

fn int(hz: u128) -> Exact<Hertz> {
    Exact::int(hz.try_into().unwrap())
}

fn max(lim: &PerVos, vos: Vos) -> u128 {
    hz(lim.max[vos.idx()])
}

/// N and FRACN for a VCO of `vco_hz` from an 8 MHz ref, plus `extra` FRACN steps; integer mode when that lands on a
/// whole N.
fn n_frac(vco_hz: u128, extra: u128) -> (u16, Option<u16>) {
    let steps = vco_hz * 8192;
    assert_eq!(
        steps % REF_HZ,
        0,
        "{vco_hz} Hz is not a whole number of FRACN steps"
    );
    let steps = steps / REF_HZ + extra;
    let (n, f) = ((steps / 8192) as u16, (steps % 8192) as u16);
    (n, (f != 0).then_some(f))
}

/// A PLL from the 8 MHz ref whose VCO is `vco_hz` (+ `extra` steps).
fn pll_at(vco_hz: u128, extra: u128, p: Option<u8>, q: Option<u8>, r: Option<u8>) -> Option<Pll> {
    let (n, fracn) = n_frac(vco_hz, extra);
    Some(Pll::from_ratios(3, n, fracn, p, q, r).unwrap())
}

/// HSE 24 MHz, no PLL, sys_ck = HSE, nothing in use: every limit far away. Tests put one clock at a limit.
fn base(vos: Vos) -> Config {
    Config {
        part: Part::parse("STM32H725IGK6").unwrap(),
        vdd: Exact::new_in::<Milli>(3300, 1),
        supply: Supply::DirectSmps,
        vos,
        cpu_boost: false,
        hse: Some(Hse {
            freq: mhz(24),
            mode: HseMode::Bypass,
        }),
        hsi_div: None,
        csi: false,
        hsi48: false,
        lse: None,
        pll_source: PllSource::Hse,
        pll: [None; 3],
        sys: Sys {
            source: SysSource::Hse,
            ..Sys::default()
        },
        kernel: Kernel::default(),
        used: vec![],
        expect: vec![],
        allow: Allow {
            vos0_lifetime: vos == Vos::Vos0,
            ..Allow::default()
        },
    }
}

/// sys_ck from PLL1 P at `sys_hz` (+ `extra` FRACN steps), with D1CPRE ÷1 and the given HPRE and APB prescalers.
fn with_sys(mut c: Config, sys_hz: u128, extra: u128, hpre: u16, ppre: u8) -> Config {
    // PLL1 DIVP 1 or even; the wide VCO starts at 192 MHz
    let mut p = 1u8;
    while sys_hz * (p as u128) < 192_000_000 {
        p = if p == 1 { 2 } else { p + 2 };
    }
    c.pll[0] = pll_at(sys_hz * p as u128, extra, Some(p), None, None);
    let ppre = Ppre::from_ratio("d1ppre", ppre).unwrap();
    c.sys = Sys {
        source: SysSource::Pll1P,
        d1cpre: Hpre::Div1,
        hpre: Hpre::from_ratio("hpre", hpre).unwrap(),
        d1ppre: ppre,
        d2ppre1: ppre,
        d2ppre2: ppre,
        d3ppre: ppre,
        timpre: false,
    };
    c
}

fn errors_at(c: &Config, key: &str) -> Vec<Finding> {
    check(c)
        .into_iter()
        .filter(|f| f.severity == Severity::Error && f.key == key)
        .collect()
}

#[track_caller]
fn clean(c: &Config) -> Planned {
    let p = plan(c).unwrap_or_else(|f| panic!("{:?}: {f:#?}", c.vos));
    assert!(p.warnings.is_empty(), "{:?}: {:#?}", c.vos, p.warnings);
    p
}

#[track_caller]
fn refused(c: &Config, key: &str, what: &str) {
    let e = errors_at(c, key);
    assert!(
        e.iter().any(|f| f.message.contains(what)),
        "{:?}: no `{what}` error at {key}: {:#?}",
        c.vos,
        check(c)
    );
}

/// The fastest tree each VOS allows without the boost: CPU at its Table 13 maximum, AXI and APB at theirs (VOS0's
/// AXI 275 MHz and APB 137.5 MHz are above CPU / 2 and / 4), flash at the wait states of Table 16.
#[test]
fn a_valid_tree_at_each_vos() {
    for (vos, ws) in [
        (Vos::Vos0, 3),
        (Vos::Vos1, 2),
        (Vos::Vos2, 2),
        (Vos::Vos3, 2),
    ] {
        let cpu = max(&limits::CPU, vos);
        let c = with_sys(base(vos), cpu, 0, 2, 2);
        let p = clean(&c);
        assert_eq!(p.tree.cpu, Some(int(cpu)), "{vos:?}");
        assert_eq!(p.tree.hclk, Some(int(cpu / 2)), "{vos:?}");
        assert_eq!(p.regs.flash_acr.latency(), ws, "{vos:?}");
        assert_eq!(p.regs.pwr_d3cr_vos, regs::vos_bits(vos));
    }
}

#[test]
fn cpu_just_over_each_vos_maximum_is_refused() {
    for vos in ALL {
        let cpu = max(&limits::CPU, vos);
        // AHB and APB well under their limits, so only the CPU is over
        let c = with_sys(base(vos), cpu, 1, 4, 4);
        refused(&c, "sys.d1cpre", &format!("the {vos:?} maximum"));
        assert_eq!(errors_at(&c, "sys.hpre").len(), 0, "{vos:?}");
    }
}

#[test]
fn axi_just_over_each_vos_maximum_is_refused() {
    for vos in ALL {
        let axi = max(&limits::AXI_AHB, vos);
        // HPRE ÷1: the CPU runs at the AXI clock, far under its own limit; APB ÷4 stays under
        let c = with_sys(base(vos), axi, 1, 1, 4);
        refused(&c, "sys.hpre", &format!("the {vos:?} maximum"));
        assert_eq!(errors_at(&c, "sys.d1cpre").len(), 0, "{vos:?}");
        assert_eq!(errors_at(&c, "sys.d2ppre1").len(), 0, "{vos:?}");
        // exactly at the limit is fine (with a wait-state band for it)
        let c = with_sys(base(vos), axi, 0, 1, 4);
        clean(&c);
    }
}

#[test]
fn apb_just_over_each_vos_maximum_is_refused() {
    for vos in ALL {
        let apb = max(&limits::APB, vos);
        let c = with_sys(base(vos), apb, 1, 1, 1);
        for key in ["sys.d1ppre", "sys.d2ppre1", "sys.d2ppre2", "sys.d3ppre"] {
            refused(&c, key, &format!("the {vos:?} maximum"));
        }
        assert_eq!(errors_at(&c, "sys.hpre").len(), 0, "{vos:?}");
        clean(&with_sys(base(vos), apb, 0, 1, 1));
    }
}

#[test]
fn wide_pll_output_just_over_each_vos_maximum_is_refused() {
    for vos in ALL {
        let out = max(&limits::PLL_OUT_WIDE, vos);
        let p = if out >= 192_000_000 { 1 } else { 2 };
        let mut c = base(vos);
        c.pll[1] = pll_at(out * p as u128, 1, Some(p), None, None);
        refused(&c, "pll2.p", &format!("the {vos:?} maximum"));
        c.pll[1] = pll_at(out * p as u128, 0, Some(p), None, None);
        clean(&c);
    }
}

#[test]
fn medium_pll_output_just_over_each_vos_maximum_is_refused() {
    for vos in ALL {
        let out = max(&limits::PLL_OUT_MEDIUM, vos);
        let mut c = base(vos);
        // ref 1 MHz (HSE ÷24): medium VCO, integer mode only; VCO = N MHz, P ÷2
        let pll = |n: u16| Some(Pll::from_ratios(24, n, None, Some(2), None, None).unwrap());
        let n = (2 * out / 1_000_000) as u16;
        c.pll[1] = pll(n + 2);
        refused(&c, "pll2.p", &format!("the {vos:?} maximum"));
        c.pll[1] = pll(n);
        let p = clean(&c);
        let o = p.tree.pll[1].unwrap();
        assert!(!o.wide, "{vos:?}");
        assert_eq!(o.p, Some(int(out)));
    }
}

#[test]
fn fdcan_kernel_just_over_each_vos_maximum_is_refused() {
    for vos in ALL {
        let k = max(&limits::kernel_max(Periph::Fdcan), vos);
        let q = 192_000_000u128.div_ceil(k) as u8;
        let mut c = base(vos);
        c.kernel.fdcan = Ker::Pll2Q;
        c.used = vec![Periph::Fdcan];
        c.pll[1] = pll_at(k * q as u128, 1, None, Some(q), None);
        refused(&c, "kernel.fdcan", &format!("the {vos:?} maximum"));
        c.pll[1] = pll_at(k * q as u128, 0, None, Some(q), None);
        clean(&c);
    }
}

/// RM0468 Table 16: the top of each band is still that band, one step more is the next band; above the last band
/// there is no setting (the AXI check refuses that clock).
#[test]
fn flash_wait_state_bands_at_each_vos() {
    let step = Exact::<Hertz>::new(15625, 16);
    for vos in ALL {
        let bands: Vec<Exact<Hertz>> = limits::FLASH_WS[vos.idx()]
            .iter()
            .flatten()
            .copied()
            .collect();
        assert_eq!(flash_wait_states(mhz(1), vos), Some(0));
        for (ws, &top) in bands.iter().enumerate() {
            assert_eq!(flash_wait_states(top, vos), Some(ws as u8), "{vos:?} {top}");
            let above = (top + step).unwrap();
            let next = (ws + 1 < bands.len()).then_some(ws as u8 + 1);
            assert_eq!(flash_wait_states(above, vos), next, "{vos:?} {above}");
        }
        // the last band ends at the AXI maximum
        assert_eq!(
            *bands.last().unwrap(),
            limits::AXI_AHB.max[vos.idx()],
            "{vos:?}"
        );
    }
}

#[test]
fn vos0_warns_with_the_an5337_numbers_unless_allowed() {
    let mut c = with_sys(base(Vos::Vos0), 520_000_000, 0, 2, 2);
    clean(&c);
    c.allow.vos0_lifetime = false;
    let p = plan(&c).expect("a warning, not an error");
    assert_eq!(p.warnings.len(), 1, "{:#?}", p.warnings);
    let w = &p.warnings[0];
    assert_eq!(w.key, "vos");
    assert!(w.message.contains("2 years at TJ 105 °C"), "{w}");
    assert!(w.to_string().contains("AN5337"), "{w}");
    // an extended-temperature part also hears about the 105 °C junction limit
    c.allow.vos0_lifetime = true;
    c.part = Part::parse("STM32H725IGK3").unwrap();
    let p = plan(&c).unwrap();
    assert!(
        p.warnings.iter().any(|w| w.message.contains("105 °C")),
        "{:#?}",
        p.warnings
    );
    // VOS0 with a direct SMPS needs VDD ≥ 2.2 V (DS13311 Table 14)
    c.part = Part::parse("STM32H725IGK6").unwrap();
    c.vdd = Exact::new_in::<Milli>(2100, 1);
    assert!(!errors_at(&c, "vdd").is_empty());
}

#[test]
fn cpu_boost_is_vos0_only_and_behind_its_allow() {
    // 550 MHz: PLL1 8 MHz × (68 + 6144/8192), P ÷1; AXI 275, APB 137.5, both at their VOS0 maximum
    let mut c = with_sys(base(Vos::Vos0), 550_000_000, 0, 2, 2);
    refused(&c, "sys.d1cpre", "the Vos0 maximum");
    c.cpu_boost = true;
    refused(&c, "cpu_boost", "allow.cpu_boost_tcm_ecc_off");
    c.allow.cpu_boost_tcm_ecc_off = true;
    let p = clean(&c);
    assert_eq!(p.tree.cpu, Some(mhz(550)));
    assert_eq!(p.regs.flash_acr.latency(), 3);
    // one step over 550 MHz is refused even with the boost
    let over = with_sys(c.clone(), 550_000_000, 1, 2, 4);
    refused(&over, "sys.d1cpre", "with boost");
    // and the boost needs VOS0
    let mut v1 = with_sys(base(Vos::Vos1), 400_000_000, 0, 2, 2);
    v1.cpu_boost = true;
    v1.allow.cpu_boost_tcm_ecc_off = true;
    refused(&v1, "cpu_boost", "needs VOS0");
}

/// B135's change to VOS0 later is a config edit: its tree at 520 MHz plans with only the allow added.
#[test]
fn vos0_is_a_config_edit() {
    let mut c = base(Vos::Vos1);
    c.allow.vos0_lifetime = false;
    let c1 = with_sys(c.clone(), 400_000_000, 0, 2, 2);
    clean(&c1);
    c.vos = Vos::Vos0;
    c.allow.vos0_lifetime = true;
    let c0 = with_sys(c, 520_000_000, 0, 2, 2);
    let p = clean(&c0);
    assert_eq!(p.regs.pwr_d3cr_vos.0, 0);
    assert_eq!(p.regs.flash_acr.latency(), 3);
}

#[test]
fn every_pll_source() {
    // HSE, HSI ÷1 (64 MHz ÷ 8) and CSI (4 MHz ÷ 1): an 8 or 4 MHz ref, 400 MHz at VOS1
    for (src, m, n, sel) in [
        (PllSource::Hse, 3, 50, 2),
        (PllSource::Hsi, 8, 50, 0),
        (PllSource::Csi, 1, 100, 1),
    ] {
        let mut c = base(Vos::Vos1);
        c.hsi_div = Some(HsiDiv::Div1);
        c.csi = true;
        c.pll_source = src;
        c = with_sys(c, 400_000_000, 0, 2, 2);
        c.pll[0] = Some(Pll::from_ratios(m, n, None, Some(1), None, None).unwrap());
        let p = clean(&c);
        assert_eq!(p.tree.cpu, Some(mhz(400)), "{src:?}");
        assert_eq!(p.regs.pllckselr.0 & 0b11, sel, "{src:?}");
    }
    // the source must be on
    let mut c = with_sys(base(Vos::Vos1), 400_000_000, 0, 2, 2);
    c.pll_source = PllSource::Csi;
    assert!(!errors_at(&c, "pll.source").is_empty());
}

#[test]
fn every_sys_ck_source_without_a_pll() {
    for (src, hsi_div, csi, want) in [
        (SysSource::Hse, None, false, mhz(24)),
        (SysSource::Hsi, Some(HsiDiv::Div1), false, mhz(64)),
        (SysSource::Hsi, Some(HsiDiv::Div8), false, mhz(8)),
        (SysSource::Csi, None, true, mhz(4)),
    ] {
        for vos in ALL {
            let mut c = base(vos);
            c.hsi_div = hsi_div;
            c.csi = csi;
            // APB ÷2: HSI 64 MHz is over VOS3's 42.5 MHz APB maximum at ÷1
            c.sys = Sys {
                source: src,
                d1ppre: Ppre::Div2,
                d2ppre1: Ppre::Div2,
                d2ppre2: Ppre::Div2,
                d3ppre: Ppre::Div2,
                ..Sys::default()
            };
            let p = clean(&c);
            assert_eq!(p.tree.sys_ck, Some(want), "{src:?} {vos:?}");
            let ws = flash_wait_states(want, vos).unwrap();
            assert_eq!(p.regs.flash_acr.latency(), ws, "{src:?} {vos:?}");
        }
    }
}

#[test]
fn integer_and_fractional_modes_wide_and_medium_vco() {
    // integer, wide
    let c = with_sys(base(Vos::Vos1), 400_000_000, 0, 2, 2);
    assert_eq!(c.pll[0].unwrap().fracn, None);
    let p = clean(&c);
    assert!(p.tree.pll[0].unwrap().wide);
    assert_eq!(p.regs.pllcfgr.0 & 0b11, 0, "FRACEN 0, VCOSEL wide");
    // fractional, wide: 398 MHz = 8 MHz × (99 + 4096/8192) ÷ 2
    let mut c = with_sys(base(Vos::Vos1), 398_000_000, 0, 2, 2);
    c.pll[0] = pll_at(796_000_000, 0, Some(2), None, None);
    assert_eq!(c.pll[0].unwrap().fracn.map(|f| f.get()), Some(4096));
    let p = clean(&c);
    assert_eq!(p.tree.cpu, Some(mhz(398)));
    assert_eq!(p.regs.pllcfgr.0 & 0b11, 0b01, "FRACEN 1, VCOSEL wide");
    // integer, medium: ref 2 MHz is still wide, 1.5 MHz is medium (RM §8.5.5)
    let mut c = with_sys(base(Vos::Vos1), 200_000_000, 0, 2, 2);
    c.pll[0] = Some(Pll::from_ratios(16, 200, None, Some(1), None, None).unwrap());
    c.pll[0].as_mut().unwrap().p = Some(divpqr("p", 2).unwrap()); // 1.5 MHz × 200 = 300 MHz VCO, P 150 MHz
    let p = clean(&c);
    assert!(!p.tree.pll[0].unwrap().wide);
    assert_eq!(p.regs.pllcfgr.0 & 0b11, 0b10, "FRACEN 0, VCOSEL medium");
    // fractional on the medium VCO is refused (DS13311 Tables 44, 46)
    c.pll[0].as_mut().unwrap().fracn = Some(fracn(100).unwrap());
    refused(&c, "pll1.fracn", "medium VCO");
}
