static EX: InterruptExecutor = InterruptExecutor::new();
static EX_OK: InterruptExecutor = InterruptExecutor::new();

bind_interrupts!(struct Irqs {
    UCPD1_2 => ucpd::InterruptHandler<UCPD1>;
    I2C1 => i2c::InterruptHandler<I2C1>;
});

#[embassy_executor::main]
async fn main(_spawner: Spawner) {
    let mut config = embassy_stm32::Config::default();
    config.time_interrupt_priority = Priority::P1;
    let p = embassy_stm32::init(config);
    interrupt::I2C1.set_priority(Priority::P3);
    interrupt::UCPD1_2.set_priority(Priority::P2);
    interrupt::TIM7.set_priority(Priority::P2);
    let high = EX.start(interrupt::TIM7);
    high.spawn(pd_task(p.UCPD1).unwrap());

    interrupt::TIM6.set_priority(Priority::P4);
    let low = EX_OK.start(interrupt::TIM6);
    low.spawn(idle_task().unwrap());
    core::future::pending::<()>().await;
}

#[embassy_executor::task]
async fn pd_task(_ucpd: Peri<'static, UCPD1>) {
    Timer::after_millis(1).await;
}

#[embassy_executor::task]
async fn idle_task() {
    Timer::after_millis(1).await;
}
