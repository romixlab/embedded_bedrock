// embedded_bedrock TPL-25: a settings save from a task on core 1; embassy-rp's flash operations pause core 1
// from core 0
#[embassy_executor::main]
async fn main(_spawner: Spawner) {
    let p = embassy_rp::init(Default::default());
    spawn_core1(p.CORE1, unsafe { &mut *addr_of_mut!(CORE1_STACK) }, move || {
        let executor1 = EXECUTOR1.init(Executor::new());
        executor1.run(|spawner| spawner.spawn(unwrap!(core1_task())));
    });
}

#[embassy_executor::task]
async fn core1_task() {
    save_settings();
}

fn save_settings() {
    let mut flash = Flash::<_, Blocking, FLASH_SIZE>::new_blocking(unsafe { FLASH::steal() });
    flash.blocking_erase(SETTINGS, SETTINGS + 4096).ok();
    flash.blocking_write(SETTINGS, &DATA).ok();
}
