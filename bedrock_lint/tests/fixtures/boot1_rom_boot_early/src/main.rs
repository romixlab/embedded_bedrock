// bedrock_rom_boot used without early(): the ROM's jump back goes unseen
#[embassy_executor::main]
async fn main(_spawner: Spawner) {
    let p = embassy_stm32::init(Default::default());
}

fn reboot_into_dfu() {
    bedrock_rom_boot::request_reboot(bedrock_rom_boot::Reboot::Rom);
}
