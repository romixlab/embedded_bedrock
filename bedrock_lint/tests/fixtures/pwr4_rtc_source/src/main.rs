// donguru_template main.rs: the boot word lives in a backup register, and embassy resets the backup domain when
// RCC_BDCR differs from config.rcc.ls
#[embassy_executor::main]
async fn main(_spawner: Spawner) {
    let mut config = embassy_stm32::Config::default();
    config.rcc.ls = LsConfig::default_lse();
    let p = embassy_stm32::init(config);
    cnt::bkp_cnt!(boots: u32);
}

fn rtc_on_lsi() {
    pac::RCC.bdcr().modify(|w| {
        w.set_rtcsel(pac::rcc::vals::Rtcsel::LSI);
        w.set_rtcen(true);
    });
}
