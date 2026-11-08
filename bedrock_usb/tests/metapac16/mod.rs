//! The checks the 16-bit peripherals (usb_v1, usb_v2, usb_v3) share: usb_v4's names (`bedrock_usb::v4::regs`) held
//! to the PAC's fields, which are the same in all three. Included by `v1_metapac.rs`, `v2_metapac.rs` and
//! `v3_metapac.rs`, each built against one chip's stm32-metapac.

use bedrock_usb::layout::RegType;
use bedrock_usb::v4::chep::Stat;
use bedrock_usb::v4::regs::{chep, cntr, daddr, fnr, istr};
use stm32_metapac::usb::regs::{Btable, Cntr, Daddr, Epr, Fnr, Istr};
use stm32_metapac::usb::vals;

fn epr(f: impl FnOnce(&mut Epr)) -> u32 {
    let mut r = Epr(0);
    f(&mut r);
    r.0
}

pub fn epr_bits() {
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
    // every field is in the low half-word, and those are all the low half-word holds
    let all = epr(|r| {
        r.set_ea(0xf);
        r.set_stat_tx(vals::Stat::VALID);
        r.set_dtog_tx(true);
        r.set_ctr_tx(true);
        r.set_ep_kind(true);
        r.set_ep_type(vals::EpType::INTERRUPT);
        r.set_setup(true);
        r.set_stat_rx(vals::Stat::VALID);
        r.set_dtog_rx(true);
        r.set_ctr_rx(true);
    });
    assert_eq!(all, 0xffff);
    // of usb_v4's rc_w0 bits only CTR_RX and CTR_TX are left in it; the host-mode ones are above
    assert_eq!(chep::RC_W0 & 0xffff, chep::VTRX | chep::VTTX);
    assert_eq!(chep::HOST_RC_W0 & 0xffff, 0);
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

pub fn cntr_istr_daddr_btable_bits() {
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

    // the frame number the SOF hook reads
    let mut f = Fnr(0);
    f.set_fn_(0x7ff);
    assert_eq!(f.0, fnr::FN);
    let mut f = Fnr(0);
    f.set_lsof(0b11);
    assert_eq!(f.0, fnr::LSOF);
    let mut f = Fnr(0);
    f.set_lck(true);
    assert_eq!(f.0, fnr::LCK);

    let mut d = Daddr(0);
    d.set_add(0x7f);
    assert_eq!(d.0, daddr::ADD);
    let mut d = Daddr(0);
    d.set_ef(true);
    assert_eq!(d.0, daddr::EF);

    // BTABLE[15:3]: the driver's 0 is offset 0, and any 8-aligned offset is the register's value as is
    let mut b = Btable(0);
    b.set_btable(0x40 >> 3);
    assert_eq!(b.0, 0x40);
}
