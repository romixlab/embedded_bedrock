#[embassy_executor::task]
async fn pump_task() {
    loop {
        let first = JOBS.receive().await;
        handle(first);
        while let Ok(job) = JOBS.try_receive() {
            handle(job);
        }
    }
}

#[embassy_executor::task]
async fn batched_task() {
    loop {
        let first = JOBS.receive().await;
        handle(first);
        let mut n = 0;
        while let Ok(job) = JOBS.try_receive() {
            handle(job);
            n += 1;
            if n == 8 {
                break;
            }
        }
        yield_now().await;
    }
}

#[embassy_executor::task]
async fn log_task() {
    loop {
        Timer::after_secs(1).await;
        let mut i = 0;
        while i < LEN {
            info!("entry {}", i);
            i += 1;
        }
    }
}

fn not_async() {
    while let Some(x) = QUEUE.pop() {
        handle(x);
    }
}
