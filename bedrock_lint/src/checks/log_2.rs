//! `LOG-2` `DEFMT_RTT_BUFFER_SIZE` and `DEFMT_LOG` not set explicitly in `.cargo/config.toml`

use crate::check::{Check, CheckMeta, Registered};
use crate::finding::{Finding, Span};
use crate::model::Firmware;

pub struct Log2;

static META: CheckMeta = CheckMeta {
    id: "LOG-2",
    needs: &[],
};

inventory::submit!(Registered(&Log2));

fn env_var(fw: &Firmware, name: &str) -> bool {
    fw.cargo_config(&["env", name]).is_some()
}

impl Check for Log2 {
    fn meta(&self) -> &'static CheckMeta {
        &META
    }

    fn run(&self, fw: &Firmware) -> Vec<Finding> {
        if !fw.dependencies().iter().any(|d| d.package() == "defmt") {
            return Vec::new();
        }
        let mut out = Vec::new();
        for (name, why) in [
            (
                "DEFMT_LOG",
                "the log level falls back to a built-in default",
            ),
            (
                "DEFMT_RTT_BUFFER_SIZE",
                "the RTT buffer falls back to defmt-rtt's default and can lose a burst (init especially)",
            ),
        ] {
            if env_var(fw, name) {
                continue;
            }
            let span = fw
                .cargo_configs
                .first()
                .map(|c| Span::new(c.rel(), c.line_of_table(&["env"])))
                .unwrap_or_else(|| Span::new(fw.manifest.rel(), 0));
            out.push(
                Finding::new(
                    "LOG-2",
                    name,
                    format!("{name} not set explicitly in [env] of .cargo/config.toml: {why}"),
                )
                .span(span)
                .fix(format!("set {name} under [env] in .cargo/config.toml")),
            );
        }
        out
    }
}
