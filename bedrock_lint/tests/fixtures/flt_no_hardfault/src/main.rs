#![no_std]
#![no_main]

// No HardFault (cortex-m-rt's spins), fault handlers never enabled, no watchdog, no boot-loop rescue

#[entry]
fn main() -> ! {
    let cause = read_reset_cause();
    loop {}
}

#[exception]
unsafe fn UsageFault() {
    record_and_reset();
}

#[exception]
unsafe fn MemoryManagement() {
    record_and_reset();
}

#[exception]
unsafe fn BusFault() {
    record_and_reset();
}

fn read_reset_cause() -> u32 {
    unsafe { RCC_CSR.read_volatile() }
}
