// typec_dongle FW-12, vb135 BOOT-6: the firmware jumps into the ROM bootloader for DFU and comes back through the
// ROM's DfuSe leave, a jump, with nothing that notices
#[embassy_executor::main]
async fn main(_spawner: Spawner) {
    let p = embassy_stm32::init(Default::default());
}

fn enter_dfu() -> ! {
    unsafe { cortex_m::asm::bootload(SYSTEM_MEMORY as *const u32) }
}
