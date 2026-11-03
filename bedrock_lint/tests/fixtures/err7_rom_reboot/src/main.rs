// template boot.rs: RP2350-E30 (delay_ms 0 doesn't reboot) and RP2350-E19 (FRCE_OFF bits beyond PROC1 hang it)
const NO_DELAY: u32 = 0;

fn reboot_to_bootsel() {
    embassy_rp::rom_data::reboot(0x0002, 0, 0, 0);
}

fn reboot_later() {
    embassy_rp::rom_data::reboot(0x0002, NO_DELAY, 0, 0);
    embassy_rp::rom_data::reboot(0x0002, 10, 0, 0);
}

fn park_cores() {
    pac::PSM.frce_off().modify(|w| {
        w.set_proc1(true);
        w.set_sio(true);
    });
}
