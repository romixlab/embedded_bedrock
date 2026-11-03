bind_interrupts!(struct Irqs {
    USB_UCPD1_2 => usb::InterruptHandler<USB>, ucpd::InterruptHandler<UCPD1>;
    DMA1_CHANNEL1 => dma::InterruptHandler<DMA1_CH1>;
});
