// typec_dongle FW-24: single-buffered bulk endpoints behind the hub's TT gave 500 kB/s instead of 947
fn endpoints(alt: &mut InterfaceAltBuilder<'_, Driver<'static, USB>>) {
    let read = alt.endpoint_bulk_out(None, 64);
    let write = alt.endpoint_bulk_in_double_buffered(None, 64);
}
