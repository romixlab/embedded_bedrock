// notion: a CRC of a USB packet is not a CRC of the image
#[embassy_executor::main]
async fn main(_spawner: Spawner) {
    let p = embassy_stm32::init(Default::default());
    let crc = packet_crc(&[1, 2, 3]);
}
