mod cortex_m {
    pub mod interrupt {
        pub fn disable() {}
        pub unsafe fn enable() {}
    }
}

struct Periph;
impl Periph {
    fn disable(&self) {}
}

// bedrock-lint: allow(IRQ-6) idle-time measurement around wfe (as typec_dongle's cpu.rs run() does), not nested
fn measure_idle() {
    cortex_m::interrupt::disable();
    // (would read a clock and `wfe` here)
    unsafe { cortex_m::interrupt::enable() };
}

fn asm_cpsid_cpsie() {
    unsafe { core::arch::asm!("cpsid i") };
    unsafe { core::arch::asm!("cpsie i") };
}

fn only_disables() {
    cortex_m::interrupt::disable();
}

fn unrelated_disable(p: &Periph) {
    p.disable();
}
