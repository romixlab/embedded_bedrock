//! The usb_v1 port's addresses, register offsets, SRAM spacing and bits against stm32-metapac's `usb_v1` and
//! `usbram_16x1_512` (STM32F103C8), the PAC embassy-stm32 drives the same peripheral with.

mod metapac16;

use bedrock_usb::v1::{Access, Mmio, Spaced512};
use bedrock_usb::v3::offset as v3_offset;
use bedrock_usb::v4::regs::offset;
use stm32_metapac::{USB, USBRAM};

fn off<T: Copy, A: stm32_metapac::common::Access>(r: stm32_metapac::common::Reg<T, A>) -> usize {
    r.as_ptr() as usize - USB.as_ptr() as usize
}

#[test]
fn addresses() {
    assert_eq!(USB.as_ptr() as usize, Mmio::<Spaced512>::USB);
    assert_eq!(USBRAM.as_ptr() as usize, Mmio::<Spaced512>::SRAM);
    for n in 0..8 {
        assert_eq!(off(USB.epr(n)), offset::CHEP0R + 4 * n);
    }
    assert_eq!(off(USB.cntr()), offset::CNTR);
    assert_eq!(off(USB.istr()), offset::ISTR);
    assert_eq!(off(USB.fnr()), offset::FNR);
    assert_eq!(off(USB.daddr()), offset::DADDR);
    assert_eq!(off(USB.btable()), v3_offset::BTABLE);
    // USB SRAM: 256 half-words, one per 32-bit word ("1 x 16 bits / word"), 512 bytes for the peripheral
    let mem = |n: usize| USBRAM.mem(n).as_ptr() as usize - USBRAM.as_ptr() as usize;
    assert_eq!(mem(1), Spaced512::STRIDE);
    assert_eq!(mem(255) / Spaced512::STRIDE * 2 + 2, Spaced512::SRAM_SIZE);
}

#[test]
fn epr_bits() {
    metapac16::epr_bits();
}

#[test]
fn cntr_istr_daddr_btable_bits() {
    metapac16::cntr_istr_daddr_btable_bits();
}
