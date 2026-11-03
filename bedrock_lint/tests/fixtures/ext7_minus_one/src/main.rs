// mcan T1: a prescaler of 0 encoded as brp - 1 underflows and lands as the field's maximum
fn bit_timing(regs: Fdcan, brp: u16, seg1: u8) {
    regs.nbtp().write(|w| {
        w.set_nbrp(brp - 1);
        w.set_ntseg1(seg1 - 1);
    });
}

fn bit_timing_checked(regs: Fdcan, brp: u16, seg1: core::num::NonZeroU8) -> Result<(), ()> {
    if brp == 0 || brp > 512 {
        return Err(());
    }
    regs.nbtp().write(|w| {
        w.set_nbrp(brp - 1);
        w.set_ntseg1(seg1.get() - 1);
    });
    Ok(())
}
