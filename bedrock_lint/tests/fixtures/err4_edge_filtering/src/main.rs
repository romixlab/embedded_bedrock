// template errata.md, vb135 PLAT-6: ES0548 2.15.1 (edge filtering) and 2.15.2 (dedicated Tx buffers beside the FIFO)
fn configure(regs: Fdcan) {
    regs.cccr().modify(|w| w.set_efbi(true));
    regs.txbc().modify(|w| {
        w.set_ndtb(2);
        w.set_tfqs(3);
    });
}

fn from_host(regs: Fdcan, edge: bool) {
    regs.cccr().modify(|w| w.set_efbi(edge));
}
