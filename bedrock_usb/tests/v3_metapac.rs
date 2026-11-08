//! The usb_v3 port's addresses, register offsets and bits against stm32-metapac's `usb_v3` and `usbram_16x2_1024`
//! (STM32G474), the PAC embassy-stm32 drives the same peripheral with. The driver's bits are usb_v4's
//! (`bedrock_usb::v4::regs`), so this holds usb_v4's names to usb_v3's fields.

mod metapac16;

use bedrock_usb::v3::{Mmio, SRAM_SIZE, offset as v3_offset};
use bedrock_usb::v4::regs::{bcdr, offset};
use stm32_metapac::usb::regs::Bcdr;
use stm32_metapac::{USB, USBRAM};

fn off<T: Copy, A: stm32_metapac::common::Access>(r: stm32_metapac::common::Reg<T, A>) -> usize {
    r.as_ptr() as usize - USB.as_ptr() as usize
}

#[test]
fn addresses() {
    assert_eq!(USB.as_ptr() as usize, Mmio::G4_USB);
    assert_eq!(USBRAM.as_ptr() as usize, Mmio::G4_SRAM);
    for n in 0..8 {
        assert_eq!(off(USB.epr(n)), offset::CHEP0R + 4 * n);
    }
    assert_eq!(off(USB.cntr()), offset::CNTR);
    assert_eq!(off(USB.istr()), offset::ISTR);
    assert_eq!(off(USB.fnr()), offset::FNR);
    assert_eq!(off(USB.daddr()), offset::DADDR);
    assert_eq!(off(USB.btable()), v3_offset::BTABLE);
    assert_eq!(off(USB.bcdr()), offset::BCDR);
    // USB SRAM: 512 half-words at consecutive addresses, 1024 bytes
    let mem = |n: usize| USBRAM.mem(n).as_ptr() as usize - USBRAM.as_ptr() as usize;
    assert_eq!(mem(1), 2);
    assert_eq!(mem(511) + 2, SRAM_SIZE);
}

#[test]
fn epr_bits() {
    metapac16::epr_bits();
}

#[test]
fn cntr_istr_daddr_btable_bcdr_bits() {
    metapac16::cntr_istr_daddr_btable_bits();
    let mut b = Bcdr(0);
    b.set_dppu(true);
    assert_eq!(b.0, bcdr::DPPU);
}
