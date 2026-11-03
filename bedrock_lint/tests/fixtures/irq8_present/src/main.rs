#[exception]
unsafe fn DefaultHandler(irqn: i16) {
    error!("unhandled exception, IRQn = {}", irqn);
}
