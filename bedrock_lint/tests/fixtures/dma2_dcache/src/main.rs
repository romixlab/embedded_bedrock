struct Peripherals {
    DMA1_CH0: u32,
}

struct Scb;
impl Scb {
    fn enable_dcache(&mut self, _cpuid: &mut u32) {}
}

struct Cp {
    SCB: Scb,
}

fn setup(p: Peripherals, cp: &mut Cp) {
    let _dma = p.DMA1_CH0;
    let mut cpuid = 0u32;
    cp.SCB.enable_dcache(&mut cpuid);
}
