static EX: InterruptExecutor = InterruptExecutor::new();

#[interrupt]
unsafe fn SWI_IRQ_0() {
    unsafe { EX.on_interrupt() }
}

#[interrupt]
fn TIMER_IRQ_1() {
    embassy_time::block_for(Duration::from_micros(5));
}

#[embassy_executor::main]
async fn main(spawner: Spawner) {
    interrupt::SWI_IRQ_0.set_priority(Priority::P1);
    let high = EX.start(interrupt::SWI_IRQ_0);
    io::start(high);
    spawner.spawn(thread_task().unwrap());
    core::future::pending::<()>().await;
}

mod io {
    pub fn start(spawner: SendSpawner) {
        spawner.spawn(sense_task().unwrap());
    }

    #[embassy_executor::task]
    async fn sense_task() {
        loop {
            unwrap!(I2C.blocking_write(0x20, &[1]));
            settle();
            Timer::after_millis(1).await;
        }
    }

    fn settle() {
        while !READY.load(Ordering::Relaxed) {}
        nb::block!(serial.read()).ok();
    }
}

#[embassy_executor::task]
async fn thread_task() {
    I2C.blocking_write(0x20, &[2]).ok();
    Timer::after_millis(1).await;
}
