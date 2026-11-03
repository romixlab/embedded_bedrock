// typec_dongle FW-27, wire_weaver STD-26: with probe-rs attached the channel turns blocking, and the firmware
// stopped whenever the host stopped reading
use defmt_rtt as _;

fn terminal() {
    let channels = rtt_init! {
        up: {
            1: { size: 512, mode: ChannelMode::BlockIfFull, name: "Terminal" }
        }
    };
}
