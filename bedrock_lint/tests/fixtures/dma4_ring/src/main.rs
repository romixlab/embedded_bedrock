#[embassy_executor::task]
async fn rx_uncounted(rx: UartRx<'static, Async>, ring: &'static mut [u8; 256]) {
    let mut rx = rx.into_ring_buffered(ring);
    let mut buf = [0; 64];
    loop {
        match rx.read(&mut buf).await {
            Ok(n) => forward(&buf[..n]),
            Err(_) => warn!("uart rx: overrun"),
        }
    }
}

#[embassy_executor::task]
async fn rx_counted(mut rx: RingBufferedUartRx<'static>) {
    let mut buf = [0; 64];
    loop {
        match rx.read(&mut buf).await {
            Ok(n) => forward(&buf[..n]),
            Err(_) => cnt::cnt!(uart_rx_errors: u32, warn),
        }
    }
}

#[embassy_executor::task]
async fn plain_task(mut rx: UartRx<'static, Async>) {
    let mut buf = [0; 64];
    loop {
        rx.read(&mut buf).await.ok();
    }
}
