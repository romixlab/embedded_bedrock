//! `g0::apply` against a simulated RM0444 chip: the RM order of the writes, bounded waits, a second apply on a running
//! PLL, HSEBYP kept through a reset, Range 2, and verify. The plan is Donguru's golden `clocks.rs` from
//! bedrock_clocks, so the generated file also compiles against this crate.

use std::collections::HashMap;

use bedrock_clocks_rt::Regs;
use bedrock_clocks_rt::g0::{self, Error, Plan, Step, addr, bits::*};

#[path = "../../bedrock_clocks/tests/golden/donguru_clocks.rs"]
mod clocks;

/// Enough of RCC / PWR / FLASH / CRS: ready flags follow their ON bits, SWS follows SW once its source is ready,
/// VOSF clears at once, CRS_CFGR is write-protected while CEN is set (RM0444 §6.7.2).
struct Sim {
    regs: HashMap<u32, u32>,
    writes: Vec<(u32, u32)>,
    hse_dead: bool,
}

impl Sim {
    /// Reset values (RM0444 §5.4, §4.4.1, §3.7.1, §6.7): HSI16 on and ready, SYSCLK HSISYS, Range 1, 0 wait states
    /// with ICEN and DBG_SWEN.
    fn new() -> Sim {
        let mut regs = HashMap::new();
        regs.insert(addr::RCC_CR, CR_HSION | CR_HSIRDY);
        regs.insert(addr::RCC_PLLCFGR, 0x0000_1000);
        regs.insert(addr::PWR_CR1, 0x0000_0208);
        regs.insert(addr::FLASH_ACR, 0x0004_0600);
        regs.insert(addr::CRS_CR, 0x0000_4000);
        regs.insert(addr::CRS_CFGR, 0x2022_BB7F);
        Sim {
            regs,
            writes: Vec::new(),
            hse_dead: false,
        }
    }

    fn first_write(&self, a: u32, pred: impl Fn(u32) -> bool) -> usize {
        self.writes
            .iter()
            .position(|&(wa, v)| wa == a && pred(v))
            .unwrap_or_else(|| panic!("no write to {a:#x}"))
    }
}

impl Regs for Sim {
    fn read(&mut self, a: u32) -> u32 {
        *self.regs.get(&a).unwrap_or(&0)
    }

    fn write(&mut self, a: u32, mut v: u32) {
        self.writes.push((a, v));
        match a {
            addr::RCC_CR => {
                let old = self.regs[&a];
                assert!(
                    (old ^ v) & CR_HSEBYP == 0 || old & CR_HSEON == 0,
                    "HSEBYP changed while HSE is on"
                );
                v &= !(CR_HSIRDY | CR_HSERDY | CR_HSI48RDY | CR_PLLRDY);
                for (on, rdy) in [
                    (CR_HSION, CR_HSIRDY),
                    (CR_HSI48ON, CR_HSI48RDY),
                    (CR_PLLON, CR_PLLRDY),
                ] {
                    if v & on != 0 {
                        v |= rdy;
                    }
                }
                if v & CR_HSEON != 0 && !self.hse_dead {
                    v |= CR_HSERDY;
                }
            }
            addr::RCC_PLLCFGR => {
                assert_eq!(
                    self.regs[&addr::RCC_CR] & CR_PLLON,
                    0,
                    "PLLCFGR written with the PLL on"
                );
            }
            addr::RCC_CFGR => {
                let sw = v & CFGR_SW;
                v = (v & !CFGR_SWS) | (sw << 3);
            }
            addr::CRS_CFGR => {
                assert_eq!(
                    self.regs[&addr::CRS_CR] & CRS_CR_CEN,
                    0,
                    "CRS_CFGR written with CEN set"
                );
            }
            _ => {}
        }
        self.regs.insert(a, v);
    }
}

#[test]
fn applies_donguru_in_rm_order() {
    let mut s = Sim::new();
    g0::apply(&mut s, &clocks::PLAN).unwrap();
    g0::verify(&mut s, &clocks::PLAN).unwrap();

    // the PLL locks before the switch, the wait states go up before it (RM0444 §3.3.4), the muxes after it
    let pll_on = s.first_write(addr::RCC_CR, |v| v & CR_PLLON != 0);
    let ws = s.first_write(addr::FLASH_ACR, |v| v & ACR_LATENCY == 2);
    let switch = s.first_write(addr::RCC_CFGR, |v| v & CFGR_SW == 2);
    let mux = s.first_write(addr::RCC_CCIPR, |_| true);
    assert!(
        pll_on < ws && ws < switch && switch < mux,
        "{:x?}",
        s.writes
    );
    // HSI48 before the PLL and the CRS, CRS_CFGR before CRS_CR
    let hsi48 = s.first_write(addr::RCC_CR, |v| v & CR_HSI48ON != 0);
    let cfgr = s.first_write(addr::CRS_CFGR, |_| true);
    let cr = s.first_write(addr::CRS_CR, |v| v & CRS_CR_CEN != 0);
    assert!(hsi48 < pll_on && switch < cfgr && cfgr < cr);
    // FLASH_ACR keeps EMPTY / DBG_SWEN as they were; PRFTEN stays off (ES0548 2.2.10)
    assert_eq!(s.regs[&addr::FLASH_ACR], 0x0004_0602);
    // the PWR and CRS clocks are on
    assert_eq!(s.regs[&addr::RCC_APBENR1], APBENR1_PWREN | APBENR1_CRSEN);
    // HSI16 stays on (UCPD, the PLL)
    assert_ne!(s.regs[&addr::RCC_CR] & CR_HSION, 0);
}

#[test]
fn second_apply_leaves_the_running_pll_first() {
    let mut s = Sim::new();
    g0::apply(&mut s, &clocks::PLAN).unwrap();
    s.writes.clear();
    g0::apply(&mut s, &clocks::PLAN).unwrap();
    g0::verify(&mut s, &clocks::PLAN).unwrap();
    let back = s.first_write(addr::RCC_CFGR, |v| v & CFGR_SW == 0);
    let off = s.first_write(addr::RCC_CR, |v| v & CR_PLLON == 0);
    let cfg = s.first_write(addr::RCC_PLLCFGR, |_| true);
    assert!(back < off && off < cfg);
}

#[test]
fn hse_bypass_survives_reset_and_hse_timeout_is_named() {
    let plan = Plan {
        rcc_cr_osc: CR_HSION | CR_HSEON,
        ..clocks::PLAN
    };
    let mut s = Sim::new();
    // HSEBYP keeps its value through a system reset (RM0444 §5.4.1), from an earlier bypass firmware
    s.regs
        .insert(addr::RCC_CR, CR_HSION | CR_HSIRDY | CR_HSEBYP);
    g0::apply(&mut s, &plan).unwrap();
    assert_eq!(s.regs[&addr::RCC_CR] & CR_HSEBYP, 0);

    let mut s = Sim::new();
    s.hse_dead = true;
    assert_eq!(
        g0::apply(&mut s, &plan),
        Err(Error::Timeout(Step::HseReady))
    );
}

#[test]
fn range2_goes_down_after_the_clock() {
    // HSISYS 16 MHz, Range 2, 1 wait state (RM0444 Table 12): VOS after the wait states and the switch
    let plan = Plan {
        pwr_cr1_vos: 0b10 << 9,
        flash_acr: 1 | (1 << 9),
        rcc_cr_osc: CR_HSION,
        rcc_cr_pll_on: 0,
        pllcfgr: 0x0000_1000,
        cfgr: 0,
        ccipr: 0,
        ccipr2: 0,
        crs_cfgr: 0,
        crs_cr: 0,
        rcc_cr_off: 0,
    };
    let mut s = Sim::new();
    g0::apply(&mut s, &plan).unwrap();
    g0::verify(&mut s, &plan).unwrap();
    let ws = s.first_write(addr::FLASH_ACR, |v| v & ACR_LATENCY == 1);
    let vos = s.first_write(addr::PWR_CR1, |v| v & PWR_CR1_VOS == 0b10 << 9);
    assert!(ws < vos);
    // and back up: Range 1 before the PLL
    g0::apply(&mut s, &clocks::PLAN).unwrap();
    g0::verify(&mut s, &clocks::PLAN).unwrap();
}

#[test]
fn verify_names_the_register() {
    let mut s = Sim::new();
    g0::apply(&mut s, &clocks::PLAN).unwrap();
    s.regs.insert(addr::RCC_CCIPR, 0);
    let m = g0::verify(&mut s, &clocks::PLAN).unwrap_err();
    assert_eq!(m.register, "RCC_CCIPR");
    assert_eq!((m.want, m.got), (1 << 30, 0));
}

#[test]
fn golden_frequencies() {
    use exact_units::q;
    assert!(clocks::SYSCLK == q!(64 MHz));
    assert!(clocks::ADC_KER_CK == q!(32 MHz));
    assert_eq!(clocks::HCLK_HZ_FLOOR, 64_000_000);
}
