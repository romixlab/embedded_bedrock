#[embassy_executor::task]
async fn rx_task(rx: UartRx<'static, Async>, ring: &'static mut [u8; 256]) {
    let mut rx = rx.into_ring_buffered(ring);
    let mut buf = [0; 64];
    loop {
        if let Ok(n) = rx.read(&mut buf).await {
            forward(&buf[..n]);
        }
    }
}

#[embassy_executor::task]
async fn gathered_task(mut rx: RingBufferedUartRx<'static>) {
    let mut buf = [0; 64];
    loop {
        match rx.read(&mut buf).await {
            Ok(n) if n < 8 => Timer::after_millis(1).await,
            Ok(_) => {}
            Err(_) => {}
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
