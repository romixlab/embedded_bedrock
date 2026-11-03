static EX_LATE: InterruptExecutor = InterruptExecutor::new();
static EX_NEVER: InterruptExecutor = InterruptExecutor::new();
static EX_HELPER: InterruptExecutor = InterruptExecutor::new();
static EX_ELSEWHERE: InterruptExecutor = InterruptExecutor::new();
static EX_WAIVED: InterruptExecutor = InterruptExecutor::new();

#[embassy_executor::main]
async fn main(_spawner: Spawner) {
    let _late = EX_LATE.start(interrupt::SWI_IRQ_0);
    interrupt::SWI_IRQ_0.set_priority(Priority::P1);

    let _never = unwrap!(EX_NEVER.start(interrupt::SWI_IRQ_1));

    priorities();
    let _helper = EX_HELPER.start(interrupt::SWI_IRQ_2);

    let _elsewhere = EX_ELSEWHERE.start(interrupt::SWI_IRQ_3);

    // bedrock-lint: allow(EXE-3) the ROM bootloader leaves SWI_IRQ_4 at P2
    let _waived = EX_WAIVED.start(interrupt::SWI_IRQ_4);
}

fn priorities() {
    interrupt::SWI_IRQ_2.set_priority(Priority::P2);
}

fn later() {
    // bedrock-lint: allow(EXE-3)
    interrupt::SWI_IRQ_3.set_priority(Priority::P3);
}
