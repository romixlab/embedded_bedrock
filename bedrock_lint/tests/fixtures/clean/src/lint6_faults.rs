// Fault handling done right (LINT-6 FLT checks): crash capture and reset, the IRQ number recorded raw, the reset
// cause read, a supervised watchdog, the boot-loop rescue

use core::sync::atomic::{AtomicBool, AtomicI16, Ordering};

static LAST_IRQ: AtomicI16 = AtomicI16::new(0);
static TASKS_ALIVE: AtomicBool = AtomicBool::new(false);

pub fn boot() {
    let early = bedrock_rom_boot::early();
    if early.start == bedrock_rom_boot::Start::Rescue {
        bedrock_rom_boot::rescue();
    }
    let _ = bedrock_crash::boot(reset_cause(), build_id());
}

#[embassy_executor::task]
pub async fn supervisor(mut wdg: IndependentWatchdog<'static>) {
    loop {
        Timer::after_millis(500).await;
        if TASKS_ALIVE.swap(false, Ordering::Relaxed) {
            wdg.pet();
        }
    }
}

#[exception]
unsafe fn DefaultHandler(irqn: i16) {
    LAST_IRQ.store(irqn, Ordering::Relaxed);
}

#[exception]
unsafe fn HardFault(ef: &cortex_m_rt::ExceptionFrame) -> ! {
    let exc_return = bedrock_crash::exc_return!();
    unsafe { bedrock_crash::capture_fault(ef, exc_return, uptime_ms()) };
    bedrock_crash::reset()
}
