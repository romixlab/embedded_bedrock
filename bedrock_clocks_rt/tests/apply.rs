//! `apply` against a simulated RM0468 chip: the RM order of the writes, the supply lock, bounded waits, a second
//! apply on a running PLL, FRACN steering, and the register addresses against stm32-metapac. The plan is B135's
//! golden `clocks.rs` from bedrock_clocks, so the generated file also compiles against this crate.

use std::collections::HashMap;

use bedrock_clocks_rt::h72x::{self, Error, Regs, Step, addr};

#[path = "../../bedrock_clocks/tests/golden/b135b_clocks.rs"]
mod clocks;

/// Enough of RCC / PWR / FLASH: ready flags follow their ON bits, SWS follows SW, PWR_CR3 locks after its first
/// write (RM0468 §6.8.4).
struct Sim {
    regs: HashMap<u32, u32>,
    writes: Vec<(u32, u32)>,
    reads: usize,
    cr3_written: bool,
    hse_dead: bool,
}

impl Sim {
    fn new() -> Sim {
        let mut regs = HashMap::new();
        // reset values (RM0468 §8.7, §6.8, §4.9): HSI on and ready, sys_ck on HSI, VOS3, 7 wait states
        regs.insert(addr::RCC_CR, 0x0000_0001 | (1 << 2) | (1 << 5));
        regs.insert(addr::RCC_PLLCKSELR, 0x0202_0200);
        regs.insert(addr::RCC_PLLCFGR, 0x01FF_0000);
        for i in 0..3 {
            regs.insert(addr::rcc_plldivr(i), 0x0101_0280);
        }
        regs.insert(addr::PWR_CR3, 0x46);
        regs.insert(addr::PWR_CSR1, (0b01 << 14) | (1 << 13));
        regs.insert(addr::PWR_D3CR, (0b01 << 14) | (1 << 13));
        regs.insert(addr::FLASH_ACR, 0x37);
        Sim {
            regs,
            writes: Vec::new(),
            reads: 0,
            cr3_written: false,
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
        self.reads += 1;
        *self.regs.get(&a).unwrap_or(&0)
    }

    fn write(&mut self, a: u32, mut v: u32) {
        self.writes.push((a, v));
        match a {
            addr::PWR_CR3 => {
                if self.cr3_written {
                    let old = self.regs[&a];
                    v = (v & !0x3F) | (old & 0x3F);
                }
                self.cr3_written = true;
            }
            addr::PWR_D3CR => {
                v |= 1 << 13;
                let csr1 = (v & (0b11 << 14)) | (1 << 13);
                self.regs.insert(addr::PWR_CSR1, csr1);
            }
            addr::RCC_CR => {
                let mut rdy = 0;
                for (on, r) in [(0, 2), (7, 8), (12, 13), (24, 25), (26, 27), (28, 29)] {
                    if v & (1 << on) != 0 {
                        rdy |= 1 << r;
                    }
                }
                if v & (1 << 16) != 0 && !self.hse_dead {
                    rdy |= 1 << 17;
                }
                let ready_bits =
                    (1 << 2) | (1 << 8) | (1 << 13) | (1 << 17) | (1 << 25) | (1 << 27) | (1 << 29);
                v = (v & !ready_bits) | rdy | (1 << 5);
            }
            addr::RCC_CFGR => {
                v = (v & !(0b111 << 3)) | ((v & 0b111) << 3);
            }
            _ => {}
        }
        self.regs.insert(a, v);
    }
}

#[test]
fn applies_in_rm_order_and_verifies() {
    let mut s = Sim::new();
    h72x::apply(&mut s, &clocks::PLAN).unwrap();
    h72x::verify(&mut s, &clocks::PLAN).unwrap();

    let cr3 = s.first_write(addr::PWR_CR3, |_| true);
    let vos = s.first_write(addr::PWR_D3CR, |_| true);
    let hse = s.first_write(addr::RCC_CR, |v| v & (1 << 16) != 0);
    let cksel = s.first_write(addr::RCC_PLLCKSELR, |_| true);
    let frac = s.first_write(addr::rcc_pllfracr(0), |_| true);
    let cfg = s.first_write(addr::RCC_PLLCFGR, |_| true);
    let div = s.first_write(addr::rcc_plldivr(0), |_| true);
    let pll_on = s.first_write(addr::RCC_CR, |v| v & (1 << 24) != 0);
    let flash = s.first_write(addr::FLASH_ACR, |_| true);
    let pre = s.first_write(addr::RCC_D1CFGR, |_| true);
    let sw = s.first_write(addr::RCC_CFGR, |v| v & 0b111 == 3);
    let mux = s.first_write(addr::RCC_D2CCIP1R, |_| true);
    let order = [
        cr3, vos, hse, cksel, frac, cfg, div, pll_on, flash, pre, sw, mux,
    ];
    assert!(order.windows(2).all(|w| w[0] < w[1]), "{order:?}");

    // HSEBYP is set before HSEON (it can only change while HSE is off, RM0468 §8.7.2)
    let byp = s.first_write(addr::RCC_CR, |v| v & (1 << 18) != 0);
    assert!(byp < hse);
    assert_eq!(s.writes[byp].1 & (1 << 16), 0);
    // HSI off at the end: the plan doesn't use it
    assert_eq!(s.regs[&addr::RCC_CR] & 1, 0);
}

#[test]
fn supply_already_written_with_the_same_value_is_fine() {
    let mut s = Sim::new();
    s.write(addr::PWR_CR3, clocks::PWR_CR3);
    s.writes.clear();
    h72x::apply(&mut s, &clocks::PLAN).unwrap();
    assert!(s.writes.iter().all(|&(a, _)| a != addr::PWR_CR3));
}

#[test]
fn supply_locked_with_another_value_is_an_error_not_a_hang() {
    let mut s = Sim::new();
    s.write(addr::PWR_CR3, 0x42); // LDO, written by someone else since power-on
    let e = h72x::apply(&mut s, &clocks::PLAN).unwrap_err();
    assert_eq!(
        e,
        Error::SupplyLocked {
            found: 0x02,
            wanted: 0x04
        }
    );
    // nothing past the supply step was touched
    assert!(s.writes.iter().all(|&(a, _)| a == addr::PWR_CR3));
}

/// HSEBYP survives a system reset (RCC-10): a plan with HSEBYP clear after a bypass firmware turns HSE off, clears
/// HSEBYP, then starts HSE again.
#[test]
fn hsebyp_left_by_an_earlier_firmware_follows_the_plan() {
    let mut s = Sim::new();
    let mut plan = clocks::PLAN;
    plan.rcc_cr_osc &= !(1 << 18); // the same tree with a crystal
    s.regs
        .insert(addr::RCC_CR, 0x0004_4025 | (1 << 16) | (1 << 17)); // HSE on in bypass
    h72x::apply(&mut s, &plan).unwrap();
    let off = s.first_write(addr::RCC_CR, |v| v & (1 << 16) == 0 && v & (1 << 18) != 0);
    let clear = s.first_write(addr::RCC_CR, |v| v & (1 << 18) == 0);
    let on = s.first_write(addr::RCC_CR, |v| v & (1 << 16) != 0 && v & (1 << 18) == 0);
    assert!(off < clear && clear <= on, "{off} {clear} {on}");
    assert_eq!(s.regs[&addr::RCC_CR] & (1 << 18), 0);
    h72x::verify(&mut s, &plan).unwrap();
    // the plan's own setting needs no HSE restart
    let mut s = Sim::new();
    s.regs.insert(addr::RCC_CR, 0x0004_4025);
    h72x::apply(&mut s, &clocks::PLAN).unwrap();
    assert!(
        !s.writes
            .iter()
            .any(|&(a, v)| a == addr::RCC_CR && v & (1 << 18) == 0)
    );
}

#[test]
fn a_dead_hse_times_out_with_its_step() {
    let mut s = Sim::new();
    s.hse_dead = true;
    assert_eq!(
        h72x::apply(&mut s, &clocks::PLAN),
        Err(Error::Timeout(Step::HseReady))
    );
}

#[test]
fn apply_twice_goes_back_to_hsi_first() {
    let mut s = Sim::new();
    h72x::apply(&mut s, &clocks::PLAN).unwrap();
    s.writes.clear();
    h72x::apply(&mut s, &clocks::PLAN).unwrap();
    h72x::verify(&mut s, &clocks::PLAN).unwrap();
    let to_hsi = s.first_write(addr::RCC_CFGR, |v| v & 0b111 == 0);
    let plls_off = s.first_write(addr::RCC_CR, |v| v & (0b10101 << 24) == 0);
    let cksel = s.first_write(addr::RCC_PLLCKSELR, |_| true);
    assert!(to_hsi < plls_off && plls_off < cksel);
}

#[test]
fn verify_names_the_register() {
    let mut s = Sim::new();
    h72x::apply(&mut s, &clocks::PLAN).unwrap();
    s.regs.insert(addr::RCC_D2CCIP1R, 0);
    let m = h72x::verify(&mut s, &clocks::PLAN).unwrap_err();
    assert_eq!(m.register, "RCC_D2CCIP1R");
    assert_eq!(m.want, clocks::RCC_D2CCIP1R);
}

#[test]
fn fracn_steps_inside_the_planned_range() {
    let mut s = Sim::new();
    h72x::apply(&mut s, &clocks::PLAN).unwrap();
    s.writes.clear();
    s.reads = 0;
    let r = clocks::PLL1_FRACN;
    h72x::set_fracn(&mut s, &r, r.max).unwrap();
    // FRACEN off, FRACN, FRACEN on (RM0468 §8.5.5 p.329)
    assert_eq!(s.writes.len(), 3);
    assert_eq!(s.writes[0].0, addr::RCC_PLLCFGR);
    assert_eq!(s.writes[0].1 & 1, 0);
    assert_eq!(s.writes[1], (addr::rcc_pllfracr(0), (r.max as u32) << 3));
    assert_eq!(s.writes[2].1 & 1, 1);
    // FRACEN held low: 2 PLL1 ref_ck periods (8 MHz) at hclk 199 MHz are 49.75 cycles, so 51 reads
    assert_eq!(r.latch_hold_reads, 51);
    assert_eq!(clocks::PLL2_FRACN.latch_hold_reads, 84);
    assert!(s.reads as u32 >= r.latch_hold_reads + 2, "{}", s.reads);
    assert!(h72x::set_fracn(&mut s, &r, r.max + 1).is_err());
    assert!(h72x::set_fracn(&mut s, &r, r.min - 1).is_err());
    // 1.227 ppm per step at B135's PLL1: 10¹² / (99·8192 + 4096) = 1 226 837.0
    assert_eq!(r.step_ppt_at(r.nominal), 1_226_837);
}

#[test]
fn addresses_match_the_pac() {
    use stm32_metapac::{FLASH, PWR, RCC};
    assert_eq!(RCC.cr().as_ptr() as u32, addr::RCC_CR);
    assert_eq!(RCC.cfgr().as_ptr() as u32, addr::RCC_CFGR);
    assert_eq!(RCC.d1cfgr().as_ptr() as u32, addr::RCC_D1CFGR);
    assert_eq!(RCC.d2cfgr().as_ptr() as u32, addr::RCC_D2CFGR);
    assert_eq!(RCC.d3cfgr().as_ptr() as u32, addr::RCC_D3CFGR);
    assert_eq!(RCC.pllckselr().as_ptr() as u32, addr::RCC_PLLCKSELR);
    assert_eq!(RCC.pllcfgr().as_ptr() as u32, addr::RCC_PLLCFGR);
    for i in 0..3 {
        assert_eq!(RCC.plldivr(i).as_ptr() as u32, addr::rcc_plldivr(i));
        assert_eq!(RCC.pllfracr(i).as_ptr() as u32, addr::rcc_pllfracr(i));
    }
    assert_eq!(RCC.d1ccipr().as_ptr() as u32, addr::RCC_D1CCIPR);
    assert_eq!(RCC.d2ccip1r().as_ptr() as u32, addr::RCC_D2CCIP1R);
    assert_eq!(RCC.d2ccip2r().as_ptr() as u32, addr::RCC_D2CCIP2R);
    assert_eq!(RCC.d3ccipr().as_ptr() as u32, addr::RCC_D3CCIPR);
    assert_eq!(PWR.csr1().as_ptr() as u32, addr::PWR_CSR1);
    assert_eq!(PWR.cr3().as_ptr() as u32, addr::PWR_CR3);
    assert_eq!(PWR.d3cr().as_ptr() as u32, addr::PWR_D3CR);
    assert_eq!(FLASH.acr().as_ptr() as u32, addr::FLASH_ACR);

    // the ready and enable bits apply() waits on
    use stm32_metapac::rcc::regs::Cr;
    let mut cr = Cr(0);
    cr.set_hsirdy(true);
    cr.set_hsidivf(true);
    cr.set_csion(true);
    cr.set_csirdy(true);
    cr.set_hsi48on(true);
    cr.set_hsi48rdy(true);
    cr.set_hseon(true);
    cr.set_hserdy(true);
    cr.set_hsebyp(true);
    for i in 0..3 {
        cr.set_pllrdy(i, true);
    }
    let ours = (1 << 2)
        | (1 << 5)
        | (1 << 7)
        | (1 << 8)
        | (1 << 12)
        | (1 << 13)
        | (1 << 16)
        | (1 << 17)
        | (1 << 18)
        | (1 << 25)
        | (1 << 27)
        | (1 << 29);
    assert_eq!(cr.0, ours);
    use stm32_metapac::pwr::regs::{Csr1, D3cr};
    let mut c = Csr1(0);
    c.set_actvosrdy(true);
    assert_eq!(c.0, 1 << 13);
    let mut d = D3cr(0);
    d.set_vosrdy(true);
    assert_eq!(d.0, 1 << 13);
}
