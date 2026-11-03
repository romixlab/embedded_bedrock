// neither a BOR level nor the PVD; naming the brown-out reset cause is not setting one
#[embassy_executor::main]
async fn main(_spawner: Spawner) {
    let p = embassy_stm32::init(Default::default());
    if last_reset() == ResetCause::Brownout {
        cnt!(brownouts: u32);
    }
}
