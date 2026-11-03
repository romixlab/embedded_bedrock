//! `EXE-10` Two crates provide the executor and `__pender`

use crate::check::{Check, CheckMeta, Registered};
use crate::finding::{Finding, Span};
use crate::model::Firmware;

pub struct Exe10;

static META: CheckMeta = CheckMeta {
    id: "EXE-10",
    needs: &[],
};

inventory::submit!(Registered(&Exe10));

const OWN_EXECUTOR_HALS: &[&str] = &["embassy-rp", "embassy-stm32", "embassy-nrf"];

impl Check for Exe10 {
    fn meta(&self) -> &'static CheckMeta {
        &META
    }

    /// `embassy-executor`'s `platform-cortex-m` provides its own `__pender`; a HAL with `executor-thread` or
    /// `executor-interrupt` on provides another
    fn run(&self, fw: &Firmware) -> Vec<Finding> {
        let deps = fw.dependencies();
        let Some(executor) = deps.iter().find(|d| d.package() == "embassy-executor") else {
            return Vec::new();
        };
        let arch_feature = executor
            .features()
            .into_iter()
            .find(|(f, _, _)| f == "platform-cortex-m");
        let Some((_, exec_file, exec_line)) = arch_feature else {
            return Vec::new();
        };
        let mut out = Vec::new();
        for d in &deps {
            if !OWN_EXECUTOR_HALS.contains(&d.package()) {
                continue;
            }
            for (feature, file, line) in d.features() {
                if feature == "executor-thread" || feature == "executor-interrupt" {
                    out.push(
                        Finding::new(
                            "EXE-10",
                            format!("{}:{}", d.file.rel(), d.package()),
                            format!(
                                "embassy-executor's platform-cortex-m and {}'s {feature} both provide __pender: \
                                 only one executor/pender pair may be linked",
                                d.package()
                            ),
                        )
                        .span(Span::new(file.rel(), line))
                        .span(Span::new(exec_file.rel(), exec_line).label("embassy-executor platform-cortex-m"))
                        .fix(format!(
                            "drop embassy-executor's platform-cortex-m feature: {} provides the executor and \
                             __pender on its own",
                            d.package()
                        )),
                    );
                }
            }
        }
        out
    }
}
