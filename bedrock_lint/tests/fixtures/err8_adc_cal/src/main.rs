// typec_dongle 0.20.7 (efton.sk gotcha 221): calibration right after a disable froze the G0's ADC
fn recalibrate(r: Adc) {
    r.cr().modify(|w| w.set_addis(true));
    while r.cr().read().aden() {}
    r.cr().modify(|w| w.set_adcal(true));
    while r.cr().read().adcal() {}
}

fn disable(r: Adc) {
    r.cr().modify(|w| w.set_addis(true));
    while r.cr().read().aden() {}
}

// right: 2 kernel clocks after the disable
fn recalibrate_waiting(r: Adc) {
    r.cr().modify(|w| w.set_addis(true));
    while r.cr().read().aden() {}
    block_for(Duration::from_micros(1));
    r.cr().modify(|w| w.set_adcal(true));
}
