//! `CLK-2` embassy time driver `time-driver-any` (the cfg half; whether the picked timer is also used for PWM or
//! capture is src mid, LINT-3)

use crate::check::{Check, CheckMeta, Registered};
use crate::finding::{Finding, Span};
use crate::model::Firmware;

pub struct Clk2;

static META: CheckMeta = CheckMeta {
    id: "CLK-2",
    needs: &[],
};

inventory::submit!(Registered(&Clk2));

impl Check for Clk2 {
    fn meta(&self) -> &'static CheckMeta {
        &META
    }

    fn run(&self, fw: &Firmware) -> Vec<Finding> {
        let mut out = Vec::new();
        for d in fw.dependencies() {
            if d.package() != "embassy-stm32" {
                continue;
            }
            for (feature, file, line) in d.features() {
                if feature == "time-driver-any" {
                    out.push(
                        Finding::new(
                            "CLK-2",
                            format!("{}:embassy-stm32:time-driver-any", file.rel()),
                            "embassy-stm32's time-driver-any lets embassy pick any available timer: check it \
                             doesn't pick one also used for PWM or capture elsewhere in the firmware",
                        )
                        .span(Span::new(file.rel(), line))
                        .fix("name the timer explicitly (`time-driver-tim5`, ...) once it is known which one PWM/capture use"),
                    );
                }
            }
        }
        out
    }
}
