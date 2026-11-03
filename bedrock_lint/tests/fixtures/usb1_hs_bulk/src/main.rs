// vb135 c0d19b8: a 64-byte bulk endpoint on the ULPI high-speed port; Windows refused the device
const MPS: u16 = 64;

fn usb(p: Peripherals) {
    let driver = Driver::new_hs_ulpi(p.USB_OTG_HS, Irqs, p.PA5, p.PC2, p.PC3, p.PC0, p.PA3, p.PB0, p.PB1, p.PB10,
        p.PB11, p.PB12, p.PB13, p.PB5, &mut EP_OUT, Default::default());
    let mut builder = Builder::new(driver, config, &mut a, &mut b, &mut c, &mut d);
    let mut func = builder.function(0xFF, 0, 0);
    let mut iface = func.interface();
    let mut alt = iface.alt_setting(0xFF, 0, 0, None);
    let ep_out = alt.endpoint_bulk_out(None, MPS);
    let ep_in = alt.endpoint_bulk_in(None, 512);
    let ep_irq = alt.endpoint_interrupt_in(None, 16, 32);
}
