#![no_std]
#![no_main]

use static_cell::StaticCell;

struct Big {
    buf: [u8; 4096],
}

static BIG: StaticCell<Big> = StaticCell::new();
static SMALL: StaticCell<u32> = StaticCell::new();

fn main() {}

fn setup() {
    let _big = BIG.init(Big { buf: [0; 4096] });
}

fn small() {
    let _s = SMALL.init(1);
}
