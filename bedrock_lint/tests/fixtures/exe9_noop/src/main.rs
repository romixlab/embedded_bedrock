fn poll_request(fut: Pin<&mut impl Future<Output = ()>>) -> bool {
    let waker = core::task::Waker::noop();
    let mut cx = Context::from_waker(waker);
    fut.poll(&mut cx).is_ready()
}

fn other() {
    let _ = embassy_futures::poll_once(async {});
    let _ = Duration::noop();
}
