struct Adc;

struct Isr {
    ccrdy: bool,
}

impl Adc {
    fn isr(&self) -> Isr {
        Isr { ccrdy: false }
    }
    fn cr(&self) -> Isr {
        Isr { ccrdy: false }
    }
}

impl Isr {
    fn modify(&self, _f: impl FnOnce(&mut Self)) {}
}

/// The G0 ADC ISR calibration-ready case: this clears every other flag set between the read and the write
fn wait_for_calibration(adc: &Adc) {
    adc.isr().modify(|w| w.ccrdy = true);
}

/// A plain configuration register is not a flag register: no finding
fn configure(adc: &Adc) {
    adc.cr().modify(|w| w.ccrdy = true);
}
