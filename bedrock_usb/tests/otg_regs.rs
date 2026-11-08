//! The OTG driver's register offsets and bits (`bedrock_usb::otg::regs`, from RM0468 Rev 3 §62.14) held to
//! embassy-usb-synopsys-otg's `otg_v1` register block, generated from the same Synopsys core: a typo in either
//! shows as a mismatch here.

use bedrock_usb::otg::regs::{
    ctl, daint, dcfg, dctl, dsts, epint, fsiz, gahbcfg, gccfg, gint, gotgctl, grstctl, grxsts,
    gusbcfg, offset, tsiz,
};
use embassy_usb_synopsys_otg::otg_v1::{Otg, regs, vals};

/// The offset of a register of `otg_v1`'s block based at 0.
fn at<T: Copy, A: embassy_usb_synopsys_otg::otg_v1::Access>(
    r: embassy_usb_synopsys_otg::otg_v1::Reg<T, A>,
) -> usize {
    r.as_ptr() as usize
}

fn otg() -> Otg {
    // Safety: only the addresses are taken, nothing is read or written
    unsafe { Otg::from_ptr(core::ptr::null_mut()) }
}

#[test]
fn offsets() {
    let o = otg();
    assert_eq!(offset::GOTGCTL, at(o.gotgctl()));
    assert_eq!(offset::GAHBCFG, at(o.gahbcfg()));
    assert_eq!(offset::GUSBCFG, at(o.gusbcfg()));
    assert_eq!(offset::GRSTCTL, at(o.grstctl()));
    assert_eq!(offset::GINTSTS, at(o.gintsts()));
    assert_eq!(offset::GINTMSK, at(o.gintmsk()));
    assert_eq!(offset::GRXSTSP, at(o.grxstsp()));
    assert_eq!(offset::GRXFSIZ, at(o.grxfsiz()));
    assert_eq!(offset::DIEPTXF0, at(o.dieptxf0()));
    assert_eq!(offset::GCCFG, at(o.gccfg_v2()));
    assert_eq!(offset::dieptxf(0), at(o.dieptxf0()));
    // otg_v1 counts the FIFOs after FIFO 0 from 0, and has 7 of them; RM0468 has OTG_DIEPTXF1..8 at
    // 0x104 + 0x04 * (x - 1)
    for x in 1..=7 {
        assert_eq!(offset::dieptxf(x), at(o.dieptxf(x - 1)), "DIEPTXF{x}");
    }
    assert_eq!(offset::dieptxf(8), 0x120);
    assert_eq!(offset::DCFG, at(o.dcfg()));
    assert_eq!(offset::DCTL, at(o.dctl()));
    assert_eq!(offset::DSTS, at(o.dsts()));
    assert_eq!(offset::DIEPMSK, at(o.diepmsk()));
    assert_eq!(offset::DOEPMSK, at(o.doepmsk()));
    assert_eq!(offset::DAINT, at(o.daint()));
    assert_eq!(offset::DAINTMSK, at(o.daintmsk()));
    for n in 0..9 {
        assert_eq!(offset::diepctl(n), at(o.diepctl(n)), "DIEPCTL{n}");
        assert_eq!(offset::diepint(n), at(o.diepint(n)), "DIEPINT{n}");
        assert_eq!(offset::dieptsiz(n), at(o.dieptsiz(n)), "DIEPTSIZ{n}");
        assert_eq!(offset::dtxfsts(n), at(o.dtxfsts(n)), "DTXFSTS{n}");
        assert_eq!(offset::doepctl(n), at(o.doepctl(n)), "DOEPCTL{n}");
        assert_eq!(offset::doepint(n), at(o.doepint(n)), "DOEPINT{n}");
        assert_eq!(offset::doeptsiz(n), at(o.doeptsiz(n)), "DOEPTSIZ{n}");
        assert_eq!(offset::fifo(n), at(o.fifo(n)), "FIFO{n}");
    }
    assert_eq!(offset::PCGCCTL, at(o.pcgcctl()));
}

/// A register value with the setters `f` applies.
fn bits<T: Default>(f: impl FnOnce(&mut T)) -> T {
    let mut v = T::default();
    f(&mut v);
    v
}

#[test]
fn global_bits() {
    let g = bits::<regs::Gotgctl>(|r| {
        r.set_bvaloen(true);
        r.set_bvaloval(true)
    });
    assert_eq!(g.0, gotgctl::BVALOEN | gotgctl::BVALOVAL);
    assert_eq!(bits::<regs::Gahbcfg>(|r| r.set_gint(true)).0, gahbcfg::GINT);
    assert_eq!(
        bits::<regs::Gusbcfg>(|r| r.set_physel(true)).0,
        gusbcfg::PHYSEL
    );
    assert_eq!(bits::<regs::Gusbcfg>(|r| r.set_trdt(0xf)).0, gusbcfg::TRDT);
    assert_eq!(
        bits::<regs::Gusbcfg>(|r| r.set_trdt(9)).0,
        9 << gusbcfg::TRDT_SHIFT
    );
    assert_eq!(
        bits::<regs::Gusbcfg>(|r| r.set_ulpifsls(true)).0,
        gusbcfg::ULPIFSLS
    );
    assert_eq!(
        bits::<regs::Gusbcfg>(|r| r.set_ulpicsm(true)).0,
        gusbcfg::ULPICSM
    );
    assert_eq!(
        bits::<regs::Gusbcfg>(|r| r.set_ulpievbusd(true)).0,
        gusbcfg::ULPIEVBUSD
    );
    assert_eq!(
        bits::<regs::Gusbcfg>(|r| r.set_ulpievbusi(true)).0,
        gusbcfg::ULPIEVBUSI
    );
    assert_eq!(
        bits::<regs::Gusbcfg>(|r| r.set_fhmod(true)).0,
        gusbcfg::FHMOD
    );
    assert_eq!(
        bits::<regs::Gusbcfg>(|r| r.set_fdmod(true)).0,
        gusbcfg::FDMOD
    );

    assert_eq!(
        bits::<regs::Grstctl>(|r| r.set_csrst(true)).0,
        grstctl::CSRST
    );
    assert_eq!(
        bits::<regs::Grstctl>(|r| r.set_rxfflsh(true)).0,
        grstctl::RXFFLSH
    );
    assert_eq!(
        bits::<regs::Grstctl>(|r| r.set_txfflsh(true)).0,
        grstctl::TXFFLSH
    );
    assert_eq!(
        bits::<regs::Grstctl>(|r| r.set_txfnum(grstctl::TXFNUM_ALL as u8)).0,
        grstctl::TXFNUM_ALL << grstctl::TXFNUM_SHIFT
    );
    assert_eq!(
        bits::<regs::Grstctl>(|r| r.set_ahbidl(true)).0,
        grstctl::AHBIDL
    );

    let gi = |f: fn(&mut regs::Gintsts)| bits::<regs::Gintsts>(f).0;
    assert_eq!(gi(|r| r.set_rxflvl(true)), gint::RXFLVL);
    assert_eq!(gi(|r| r.set_goutnakeff(true)), gint::GONAKEFF);
    assert_eq!(gi(|r| r.set_esusp(true)), gint::ESUSP);
    assert_eq!(gi(|r| r.set_usbsusp(true)), gint::USBSUSP);
    assert_eq!(gi(|r| r.set_usbrst(true)), gint::USBRST);
    assert_eq!(gi(|r| r.set_enumdne(true)), gint::ENUMDNE);
    assert_eq!(gi(|r| r.set_iepint(true)), gint::IEPINT);
    assert_eq!(gi(|r| r.set_oepint(true)), gint::OEPINT);
    assert_eq!(gi(|r| r.set_wkupint(true)), gint::WKUPINT);

    let s = bits::<regs::Grxsts>(|r| {
        r.set_epnum(0xf);
        r.set_bcnt(0x7ff)
    });
    assert_eq!(s.0, grxsts::EPNUM | grxsts::BCNT);
    let pkt =
        |v: vals::Pktstsd| bits::<regs::Grxsts>(|r| r.set_pktstsd(v)).0 >> grxsts::PKTSTS_SHIFT;
    assert_eq!(
        pkt(vals::Pktstsd::from_bits(0xf)) << grxsts::PKTSTS_SHIFT,
        grxsts::PKTSTS
    );
    assert_eq!(pkt(vals::Pktstsd::OUT_NAK), grxsts::GLOBAL_OUT_NAK);
    assert_eq!(pkt(vals::Pktstsd::OUT_DATA_RX), grxsts::OUT_DATA);
    assert_eq!(pkt(vals::Pktstsd::OUT_DATA_DONE), grxsts::OUT_DONE);
    assert_eq!(pkt(vals::Pktstsd::SETUP_DATA_DONE), grxsts::SETUP_DONE);
    assert_eq!(pkt(vals::Pktstsd::SETUP_DATA_RX), grxsts::SETUP_DATA);

    let f = bits::<regs::Fsiz>(|r| {
        r.set_sa(0x1234);
        r.set_fd(0x0567)
    });
    assert_eq!(f.0, fsiz::value(0x1234, 0x0567));
    assert_eq!(
        bits::<regs::Grxfsiz>(|r| r.set_rxfd(300)).0,
        300,
        "GRXFSIZ: the depth alone"
    );

    assert_eq!(
        bits::<regs::GccfgV2>(|r| r.set_pwrdwn(true)).0,
        gccfg::PWRDWN
    );
    assert_eq!(bits::<regs::GccfgV2>(|r| r.set_vbden(true)).0, gccfg::VBDEN);
}

#[test]
fn device_bits() {
    let sp = |v: vals::Dspd| bits::<regs::Dcfg>(|r| r.set_dspd(v)).0;
    assert_eq!(sp(vals::Dspd::FULL_SPEED_INTERNAL), dcfg::DSPD);
    assert_eq!(sp(vals::Dspd::HIGH_SPEED), dcfg::DSPD_HIGH);
    assert_eq!(sp(vals::Dspd::FULL_SPEED_EXTERNAL), dcfg::DSPD_FULL_ULPI);
    assert_eq!(
        sp(vals::Dspd::FULL_SPEED_INTERNAL),
        dcfg::DSPD_FULL_INTERNAL
    );
    assert_eq!(bits::<regs::Dcfg>(|r| r.set_dad(0x7f)).0, dcfg::DAD);
    assert_eq!(
        bits::<regs::Dcfg>(|r| r.set_pfivl(vals::Pfivl::from_bits(3))).0,
        dcfg::PFIVL
    );
    assert_eq!(bits::<regs::Dcfg>(|r| r.set_xcvrdly(true)).0, dcfg::XCVRDLY);

    assert_eq!(bits::<regs::Dctl>(|r| r.set_sdis(true)).0, dctl::SDIS);
    assert_eq!(bits::<regs::Dctl>(|r| r.set_sgonak(true)).0, dctl::SGONAK);
    assert_eq!(bits::<regs::Dctl>(|r| r.set_cgonak(true)).0, dctl::CGONAK);

    assert_eq!(bits::<regs::Dsts>(|r| r.set_suspsts(true)).0, dsts::SUSPSTS);
    assert_eq!(
        bits::<regs::Dsts>(|r| r.set_enumspd(vals::Dspd::FULL_SPEED_INTERNAL)).0,
        dsts::ENUMSPD
    );
    assert_eq!(vals::Dspd::HIGH_SPEED.to_bits() as u32, dsts::ENUMSPD_HIGH);

    let d = bits::<regs::Daint>(|r| {
        r.set_iepint(1 << 3);
        r.set_oepint(1 << 5)
    });
    assert_eq!(d.0, daint::in_ep(3) | daint::out_ep(5));
}

#[test]
fn endpoint_bits() {
    // DIEPCTLx and DOEPCTLx share their bits
    macro_rules! ctl_bits {
        ($t:ty) => {
            let c = |f: fn(&mut $t)| bits::<$t>(f).0;
            assert_eq!(c(|r| r.set_mpsiz(0x7ff)), ctl::MPSIZ);
            assert_eq!(c(|r| r.set_usbaep(true)), ctl::USBAEP);
            assert_eq!(c(|r| r.set_eonum_dpid(true)), ctl::DPID);
            assert_eq!(c(|r| r.set_naksts(true)), ctl::NAKSTS);
            assert_eq!(c(|r| r.set_eptyp(vals::Eptyp::INTERRUPT)), ctl::EPTYP);
            assert_eq!(c(|r| r.set_eptyp(vals::Eptyp::BULK)), ctl::EPTYP_BULK);
            assert_eq!(
                c(|r| r.set_eptyp(vals::Eptyp::INTERRUPT)),
                ctl::EPTYP_INTERRUPT
            );
            assert_eq!(c(|r| r.set_stall(true)), ctl::STALL);
            assert_eq!(c(|r| r.set_cnak(true)), ctl::CNAK);
            assert_eq!(c(|r| r.set_snak(true)), ctl::SNAK);
            assert_eq!(c(|r| r.set_sd0pid_sevnfrm(true)), ctl::SD0PID);
            assert_eq!(c(|r| r.set_epdis(true)), ctl::EPDIS);
            assert_eq!(c(|r| r.set_epena(true)), ctl::EPENA);
        };
    }
    ctl_bits!(regs::Diepctl);
    ctl_bits!(regs::Doepctl);
    assert_eq!(bits::<regs::Diepctl>(|r| r.set_txfnum(0xf)).0, ctl::TXFNUM);

    let i = |f: fn(&mut regs::Diepint)| bits::<regs::Diepint>(f).0;
    assert_eq!(i(|r| r.set_xfrc(true)), epint::XFRC);
    assert_eq!(i(|r| r.set_epdisd(true)), epint::EPDISD);
    assert_eq!(i(|r| r.set_toc(true)), epint::TOC);
    assert_eq!(i(|r| r.set_inepne(true)), epint::INEPNE);
    let o = |f: fn(&mut regs::Doepint)| bits::<regs::Doepint>(f).0;
    assert_eq!(o(|r| r.set_xfrc(true)), epint::XFRC);
    assert_eq!(o(|r| r.set_epdisd(true)), epint::EPDISD);
    assert_eq!(o(|r| r.set_stup(true)), epint::STUP);

    let t = bits::<regs::Dieptsiz>(|r| {
        r.set_xfrsiz(0x7_ffff);
        r.set_pktcnt(0x3ff)
    });
    assert_eq!(t.0, tsiz::XFRSIZ | tsiz::PKTCNT);
    let t = bits::<regs::Doeptsiz>(|r| {
        r.set_xfrsiz(0x7_ffff);
        r.set_pktcnt(0x3ff);
        r.set_rxdpid_stupcnt(3)
    });
    assert_eq!(t.0, tsiz::XFRSIZ | tsiz::PKTCNT | tsiz::STUPCNT);
    let one = bits::<regs::Doeptsiz>(|r| {
        r.set_xfrsiz(512);
        r.set_pktcnt(1)
    });
    assert_eq!(one.0, tsiz::one(512));
}
