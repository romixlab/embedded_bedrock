// template stm32-notes: no RTC, no backup registers, and the backup domain is never reset
#[embassy_executor::main]
async fn main(_spawner: Spawner) {
    let p = embassy_stm32::init(Default::default());
}
