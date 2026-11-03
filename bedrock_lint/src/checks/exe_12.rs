//! `EXE-12` `async fn` that never awaits

use crate::check::{Check, CheckMeta, Need, Registered};
use crate::finding::Finding;
use crate::model::Firmware;

pub struct Exe12;

static META: CheckMeta = CheckMeta {
    id: "EXE-12",
    needs: &[Need::Sources],
};

inventory::submit!(Registered(&Exe12));

impl Check for Exe12 {
    fn meta(&self) -> &'static CheckMeta {
        &META
    }

    /// Every `async fn` with a body without an `await` token (macro bodies included): certain
    fn run(&self, fw: &Firmware) -> Vec<Finding> {
        fw.facts
            .functions
            .iter()
            .filter(|f| f.is_async && !f.awaits)
            .map(|f| {
                Finding::new(
                    "EXE-12",
                    f.path.clone(),
                    format!(
                        "`async fn {}` never awaits: its state machine costs flash in every future that calls it",
                        f.name
                    ),
                )
                .span(f.loc.span())
                .fix("make it a plain `fn` and drop the `.await` at its callers (a trait method that must be async: waive it)")
            })
            .collect()
    }
}
