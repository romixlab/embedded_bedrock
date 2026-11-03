#![no_std]
#![no_main]

#[unsafe(link_section = ".rodata.signature")]
static mut SIGNATURE: u32 = 0x5157_4e43;

fn main() {}
