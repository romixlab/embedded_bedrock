//! The usb_v4 driver's register offsets and bits against stm32-metapac's `usb_v4` and `usbram_32_2048` (STM32G0B1),
//! the PAC embassy-stm32 drives the same peripheral with.

use bedrock_usb::layout::RegType;
use bedrock_usb::v4::chep::Stat;
use bedrock_usb::v4::regs::{Mmio, bcdr, chep, cntr, daddr, istr, offset};
use stm32_metapac::usb::regs::{Bcdr, Cntr, Daddr, Epr, Istr};
use stm32_metapac::usb::vals;
use stm32_metapac::{USB, USBRAM};

fn off<T: Copy, A: stm32_metapac::common::Access>(r: stm32_metapac::common::Reg<T, A>) -> usize {
    r.as_ptr() as usize - USB.as_ptr() as usize
}

#[test]
fn addresses() {
    assert_eq!(USB.as_ptr() as usize, Mmio::G0_USB);
    assert_eq!(USBRAM.as_ptr() as usize, Mmio::G0_SRAM);
    for n in 0..8 {
        assert_eq!(off(USB.epr(n)), offset::CHEP0R + 4 * n);
    }
    assert_eq!(off(USB.cntr()), offset::CNTR);
    assert_eq!(off(USB.istr()), offset::ISTR);
    assert_eq!(off(USB.fnr()), offset::FNR);
    assert_eq!(off(USB.daddr()), offset::DADDR);
    assert_eq!(off(USB.bcdr()), offset::BCDR);
    // USB SRAM: 512 words of 32 bits
    assert_eq!(
        USBRAM.mem(1).as_ptr() as usize - USBRAM.as_ptr() as usize,
        4
    );
    assert_eq!(
        USBRAM.mem(511).as_ptr() as usize - USBRAM.as_ptr() as usize,
        2044
    );
}

fn epr(f: impl FnOnce(&mut Epr)) -> u32 {
    let mut r = Epr(0);
    f(&mut r);
    r.0
}

#[test]
fn chep_bits() {
    assert_eq!(epr(|r| r.set_ea(0xf)), chep::EA);
    assert_eq!(epr(|r| r.set_stat_tx(vals::Stat::VALID)), chep::STATTX);
    assert_eq!(epr(|r| r.set_dtog_tx(true)), chep::DTOGTX);
    assert_eq!(epr(|r| r.set_ctr_tx(true)), chep::VTTX);
    assert_eq!(epr(|r| r.set_ep_kind(true)), chep::EPKIND);
    assert_eq!(epr(|r| r.set_ep_type(vals::EpType::INTERRUPT)), chep::UTYPE);
    assert_eq!(epr(|r| r.set_setup(true)), chep::SETUP);
    assert_eq!(epr(|r| r.set_stat_rx(vals::Stat::VALID)), chep::STATRX);
    assert_eq!(epr(|r| r.set_dtog_rx(true)), chep::DTOGRX);
    assert_eq!(epr(|r| r.set_ctr_rx(true)), chep::VTRX);
    // the host-mode rc_w0 bits: NAK, ERR_TX, ERR_RX (THREE_ERR_* are not in the PAC; RM0444 §37.6.7 bits 30:27)
    assert_eq!(epr(|r| r.set_nak(true)) & chep::HOST_RC_W0, 1 << 23);
    assert_eq!(epr(|r| r.set_err_tx(true)) & chep::HOST_RC_W0, 1 << 25);
    assert_eq!(epr(|r| r.set_err_rx(true)) & chep::HOST_RC_W0, 1 << 26);
    // STAT and UTYPE encodings
    for (ours, pac) in [
        (Stat::Disabled, vals::Stat::DISABLED),
        (Stat::Stall, vals::Stat::STALL),
        (Stat::Nak, vals::Stat::NAK),
        (Stat::Valid, vals::Stat::VALID),
    ] {
        assert_eq!(ours as u8, pac.to_bits());
    }
    assert_eq!(RegType::Bulk.utype() as u8, vals::EpType::BULK.to_bits());
    assert_eq!(
        RegType::Control.utype() as u8,
        vals::EpType::CONTROL.to_bits()
    );
    assert_eq!(
        RegType::Interrupt.utype() as u8,
        vals::EpType::INTERRUPT.to_bits()
    );
}

#[test]
fn cntr_istr_daddr_bcdr_bits() {
    let c = |f: fn(&mut Cntr)| {
        let mut r = Cntr(0);
        f(&mut r);
        r.0
    };
    assert_eq!(c(|r| r.set_fres(true)), cntr::USBRST);
    assert_eq!(c(|r| r.set_pdwn(true)), cntr::PDWN);
    assert_eq!(c(|r| r.set_lpmode(true)), cntr::SUSPRDY);
    assert_eq!(c(|r| r.set_fsusp(true)), cntr::SUSPEN);
    assert_eq!(c(|r| r.set_resume(true)), cntr::L2RES);
    assert_eq!(c(|r| r.set_esofm(true)), cntr::ESOFM);
    assert_eq!(c(|r| r.set_sofm(true)), cntr::SOFM);
    assert_eq!(c(|r| r.set_resetm(true)), cntr::RST_DCONM);
    assert_eq!(c(|r| r.set_suspm(true)), cntr::SUSPM);
    assert_eq!(c(|r| r.set_wkupm(true)), cntr::WKUPM);
    assert_eq!(c(|r| r.set_errm(true)), cntr::ERRM);
    assert_eq!(c(|r| r.set_pmaovrm(true)), cntr::PMAOVRM);
    assert_eq!(c(|r| r.set_ctrm(true)), cntr::CTRM);

    let i = |f: fn(&mut Istr)| {
        let mut r = Istr(0);
        f(&mut r);
        r.0
    };
    assert_eq!(i(|r| r.set_ep_id(0xf)), istr::IDN);
    assert_eq!(i(|r| r.set_dir(vals::Dir::FROM)), istr::DIR);
    assert_eq!(i(|r| r.set_esof(true)), istr::ESOF);
    assert_eq!(i(|r| r.set_sof(true)), istr::SOF);
    assert_eq!(i(|r| r.set_reset(true)), istr::RST_DCON);
    assert_eq!(i(|r| r.set_susp(true)), istr::SUSP);
    assert_eq!(i(|r| r.set_wkup(true)), istr::WKUP);
    assert_eq!(i(|r| r.set_err(true)), istr::ERR);
    assert_eq!(i(|r| r.set_pmaovr(true)), istr::PMAOVR);
    assert_eq!(i(|r| r.set_ctr(true)), istr::CTR);

    let mut d = Daddr(0);
    d.set_add(0x7f);
    assert_eq!(d.0, daddr::ADD);
    let mut d = Daddr(0);
    d.set_ef(true);
    assert_eq!(d.0, daddr::EF);

    let mut b = Bcdr(0);
    b.set_dppu(true);
    assert_eq!(b.0, bcdr::DPPU);
}
