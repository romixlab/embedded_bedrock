static EX_BOUND: InterruptExecutor = InterruptExecutor::new();
static EX_USED: InterruptExecutor = InterruptExecutor::new();
static EX_FREE: InterruptExecutor = InterruptExecutor::new();

bind_interrupts!(struct Irqs {
    USART3_4_5_6_LPUART1 => usart::InterruptHandler<USART5>;
});

#[embassy_executor::main]
async fn main(_spawner: Spawner) {
    let p = embassy_stm32::init(Default::default());
    interrupt::USART3_4_5_6_LPUART1.set_priority(Priority::P1);
    let _a = EX_BOUND.start(interrupt::USART3_4_5_6_LPUART1);
    interrupt::TIM7_LPTIM2.set_priority(Priority::P1);
    let _b = EX_USED.start(interrupt::TIM7_LPTIM2);
    let _pwm = SimplePwm::new(p.TIM7);
    // SWI-like: TIM14 is not used anywhere
    interrupt::TIM14.set_priority(Priority::P1);
    let _c = EX_FREE.start(interrupt::TIM14);
    core::future::pending::<()>().await;
}
