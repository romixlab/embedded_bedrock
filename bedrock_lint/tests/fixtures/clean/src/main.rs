#![no_std]
#![no_main]

static EXECUTOR_HIGH: InterruptExecutor = InterruptExecutor::new();

#[interrupt]
unsafe fn TIM7_LPTIM2() {
    unsafe { EXECUTOR_HIGH.on_interrupt() }
}

#[embassy_executor::main]
async fn main(spawner: Spawner) {
    let p = embassy_stm32::init(Default::default());
    interrupt::TIM7_LPTIM2.set_priority(Priority::P1);
    let high = EXECUTOR_HIGH.start(interrupt::TIM7_LPTIM2);
    high.spawn(pd_task(p.UCPD1).unwrap());
    spawner.spawn(usb_task().unwrap());
}

#[embassy_executor::task]
async fn pd_task(ucpd: Peri<'static, UCPD1>) {}

#[embassy_executor::task]
async fn usb_task() {}
