#![no_std]
#![no_main]

static mut SD_DMA: [u32; 128] = [0; 128];
static mut RX_BUF: [u8; 256] = [0; 256];
static mut COUNT: u32 = 0;
#[unsafe(link_section = ".axisram")]
static mut TX_DMA: [u32; 128] = [0; 128];

fn main() {}
