#[embassy_executor::main]
async fn main(spawner: Spawner) {
    let p = embassy_stm32::init(Default::default());
    let (phy, _) = Ucpd::new(p.UCPD1, Irqs).split_pd_phy(p.DMA1_CH1, p.DMA1_CH2);
    spawner.spawn(pd_task(phy).unwrap());
    spawner.spawn(usb_task(usb_device()).unwrap());
    core::future::pending::<()>().await;
}

#[embassy_executor::task]
async fn pd_task(phy: PdPhy<'static, UCPD1>) {
    loop {
        reply(&phy).await;
    }
}

async fn reply(phy: &PdPhy<'static, UCPD1>) {
    let mut buf = [0; 30];
    let _ = phy.receive(&mut buf).await;
}

#[embassy_executor::task]
async fn usb_task(mut usb: UsbDevice<'static, Driver>) {
    usb.run().await;
}
