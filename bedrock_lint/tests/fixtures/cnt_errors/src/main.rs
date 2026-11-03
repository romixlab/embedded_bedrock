#![no_std]
#![no_main]

// Error branches: typec_dongle FW-62 (a PD receive overrun that was only logged); the cnt crate's 0.4 soundness
// fix (a `&mut` to the counter buffer, a u64 through an unaligned pointer)

#[embassy_executor::main]
async fn main(_spawner: Spawner) {
    match pd_rx().await {
        Ok(msg) => handle(msg),
        Err(e) => warn!("pd rx: {}", e),
    }
    if let Err(_) = hub_configure() {}
    let Ok(n) = uart_write(b"x") else {
        return;
    };
    match select(rx(), Timer::after_millis(10)).await {
        Either::First(f) => handle(f),
        Either::Second(_) => {}
    }
    if sd_read().is_err() {
        retry_later();
    }
    match pd_rx().await {
        Ok(msg) => handle(msg),
        Err(Error::Overrun) => cnt!(pd_rx_overrun),
        Err(nb::Error::WouldBlock) => {}
        Err(e) => return Err(e.into()),
    }
    if let Err(e) = spi_write() {
        failed(e);
    }
    // bedrock-lint: allow(CNT-1) the host reverted the trial on purpose: nothing unexpected
    if let Err(_) = confirm() {}
    let _ = n;
}

fn failed(e: Error) {
    cnt!(spi_write_failed);
    let _ = e;
}

fn dump() {
    let words = unsafe { &mut *core::ptr::addr_of_mut!(_CNT_RAM_BUFFER) };
    let total = unsafe { *(cnt::counters_ram_buffer().as_ptr().add(3) as *const u64) };
    let fine: &[AtomicU32] = cnt::counters_ram_buffer();
    let _ = (words, total, fine);
}
