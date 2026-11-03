#![no_std]
#![no_main]

unsafe extern "C" {
    static mut __sram2_start: u32;
    static mut __sram2_end: u32;
}

fn main() {}
