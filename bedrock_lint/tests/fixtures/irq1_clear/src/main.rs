struct Regs;

impl Regs {
    fn icr(&self) -> Reg {
        Reg
    }
    fn ifcr(&self) -> Reg {
        Reg
    }
    fn isr(&self) -> Reg {
        Reg
    }
    fn sr(&self) -> Reg {
        Reg
    }
    fn cr(&self) -> Reg {
        Reg
    }
}

struct Reg;

impl Reg {
    fn write_value(&self, _v: u32) {}
    fn read(&self) -> u32 {
        0
    }
}

unsafe fn write_volatile(p: *mut u32, v: u32) {
    unsafe { core::ptr::write_volatile(p, v) }
}
unsafe fn read_volatile(p: *mut u32) -> u32 {
    unsafe { core::ptr::read_volatile(p) }
}

fn clear_all_ones(regs: Regs) {
    regs.icr().write_value(0xFFFF_FFFF);
}

fn clear_not(regs: Regs) {
    regs.isr().write_value(!0);
}

fn clear_max(regs: Regs) {
    regs.sr().write_value(u32::MAX);
}

fn clear_readback(regs: Regs) {
    regs.ifcr().write_value(regs.ifcr().read());
}

fn clear_via_ptr(p: *mut u32) {
    unsafe { write_volatile(p, read_volatile(p)) };
}

fn waived_boot_clear(regs: Regs) {
    // bedrock-lint: allow(IRQ-1) the ROM bootloader expects ICR cleared whole at boot
    regs.icr().write_value(0xFFFF_FFFF);
}

fn clear_one_flag(regs: Regs) {
    regs.icr().write_value(1 << 3);
}

fn unrelated_register(cfg: Regs) {
    cfg.cr().write_value(0xFFFF_FFFF);
}
