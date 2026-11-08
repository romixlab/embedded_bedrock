//! The usb_v2 parts (STM32F303xD/E) against stm32-metapac's `usb_v2` and `usbram_16x2_1024` (STM32F303RE):
//! addresses and offsets as on usb_v1, LPMCSR at 0x54 (which the driver leaves alone), the SRAM packed.

mod metapac16;

use bedrock_usb::v1::{Access, Mmio, Packed1024};
use bedrock_usb::v3::offset as v3_offset;
use bedrock_usb::v4::regs::offset;
use stm32_metapac::{USB, USBRAM};

fn off<T: Copy, A: stm32_metapac::common::Access>(r: stm32_metapac::common::Reg<T, A>) -> usize {
    r.as_ptr() as usize - USB.as_ptr() as usize
}

#[test]
fn addresses() {
    assert_eq!(USB.as_ptr() as usize, Mmio::<Packed1024>::USB);
    assert_eq!(USBRAM.as_ptr() as usize, Mmio::<Packed1024>::SRAM);
    for n in 0..8 {
        assert_eq!(off(USB.epr(n)), offset::CHEP0R + 4 * n);
    }
    assert_eq!(off(USB.cntr()), offset::CNTR);
    assert_eq!(off(USB.istr()), offset::ISTR);
    assert_eq!(off(USB.daddr()), offset::DADDR);
    assert_eq!(off(USB.btable()), v3_offset::BTABLE);
    // usb_v2's LPM register, between BTABLE and where usb_v3 has BCDR
    assert_eq!(off(USB.lpmcsr()), 0x54);
    // 512 half-words at consecutive addresses, 1024 bytes
    let mem = |n: usize| USBRAM.mem(n).as_ptr() as usize - USBRAM.as_ptr() as usize;
    assert_eq!(mem(1), Packed1024::STRIDE);
    assert_eq!(mem(511) + 2, Packed1024::SRAM_SIZE);
}

#[test]
fn epr_bits() {
    metapac16::epr_bits();
}

#[test]
fn cntr_istr_daddr_btable_bits() {
    metapac16::cntr_istr_daddr_btable_bits();
}
