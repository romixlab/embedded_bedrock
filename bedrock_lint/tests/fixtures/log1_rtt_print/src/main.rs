// defmt-rtt with blocking mode off is fine; rtt_init_print! leaves the mode to the probe
use defmt_rtt as _;

#[entry]
fn main() -> ! {
    rtt_init_print!();
    loop {}
}
