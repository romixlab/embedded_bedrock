//! The log as the template does it (LOG-1): ww_defmt_logger writes whole frames or skips them, whatever mode the
//! probe sets

ww_defmt_logger::global_logger!(LOG, 4096);

pub fn init_rtt() {
    let channels = rtt_init! {
        up: {
            0: { size: 1024, mode: NoBlockSkip, name: "defmt" }
        }
    };
    LOG.set_rtt_channel(channels.up.0);
}
