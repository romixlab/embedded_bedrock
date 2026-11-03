// pd_bench AGENTS.md: external chip registers are written from shadow copies, never read-modify-written
fn set_mode(i2c: &mut I2c, mode: u8) {
    let mut buf = [0u8];
    i2c.write_read(ADDR, &[REG_CTRL], &mut buf).ok();
    i2c.write(ADDR, &[REG_CTRL, (buf[0] & !0x03) | mode]).ok();
}

fn set_mode_shadowed(i2c: &mut I2c, shadow: &mut u8, mode: u8) {
    *shadow = (*shadow & !0x03) | mode;
    i2c.write(ADDR, &[REG_CTRL, *shadow]).ok();
}
