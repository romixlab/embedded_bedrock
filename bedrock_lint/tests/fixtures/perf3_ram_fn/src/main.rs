#![no_std]
#![no_main]

#[unsafe(link_section = ".data.fast_isr")]
fn fast_isr() {}

#[inline(never)]
#[unsafe(link_section = ".data.kept")]
fn kept() {}

fn main() {
    fast_isr();
    kept();
}
