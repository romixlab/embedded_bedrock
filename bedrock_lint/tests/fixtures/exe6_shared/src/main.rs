static EX: InterruptExecutor = InterruptExecutor::new();
static SHARED: Mutex<ThreadModeRawMutex, u32> = Mutex::new(0);
static FINE: Mutex<CriticalSectionRawMutex, u32> = Mutex::new(0);
static mut COUNT: u32 = 0;
static THREAD_ONLY: Mutex<NoopRawMutex, u32> = Mutex::new(0);

#[interrupt]
unsafe fn SWI_IRQ_0() {
    unsafe { EX.on_interrupt() }
}

#[embassy_executor::main]
async fn main(spawner: Spawner) {
    interrupt::SWI_IRQ_0.set_priority(Priority::P1);
    let high = EX.start(interrupt::SWI_IRQ_0);
    high.spawn(fast_task().unwrap());
    spawner.spawn(slow_task().unwrap());
    SHARED.lock(|v| *v);
    core::future::pending::<()>().await;
}

#[embassy_executor::task]
async fn fast_task() {
    loop {
        SHARED.lock(|v| *v);
        FINE.lock(|v| *v);
        bump();
        Timer::after_millis(1).await;
    }
}

#[embassy_executor::task]
async fn slow_task() {
    loop {
        FINE.lock(|v| *v);
        bump();
        THREAD_ONLY.lock(|v| *v);
        Timer::after_millis(1).await;
    }
}

fn bump() {
    unsafe { COUNT += 1 };
}
