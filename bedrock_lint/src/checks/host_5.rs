//! `HOST-5` Truncated or oversized request not counted and dropped

use super::host::{self, EventKind, Strength};
use crate::check::{Check, CheckMeta, Need, Registered};
use crate::finding::Finding;
use crate::model::Firmware;

pub struct Host5;

static META: CheckMeta = CheckMeta {
    id: "HOST-5",
    needs: &[Need::Sources],
};

inventory::submit!(Registered(&Host5));

impl Check for Host5 {
    fn meta(&self) -> &'static CheckMeta {
        &META
    }

    /// In a handler: an `if` comparing a host value (`data.len() < 4`), a `let .. else` on one, or a `None` /
    /// `Err` arm of a `match` on one, whose branch leaves (`return`, `continue`, `break`, an error value) without a
    /// `cnt!` counter. An estimate when the request is dropped silently, a candidate when the host gets an error back.
    fn run(&self, fw: &Firmware) -> Vec<Finding> {
        host::analyze(fw)
            .into_iter()
            .filter_map(|e| {
                let EventKind::Guard { answers } = e.kind else {
                    return None;
                };
                let (msg, s) = if answers {
                    (
                        format!(
                            "a bad request (`{}`) is answered with an error but not counted ({})",
                            e.code,
                            e.source()
                        ),
                        Strength::Candidate,
                    )
                } else {
                    (
                        format!(
                            "a truncated or oversized request (`{}`) is dropped without a counter ({})",
                            e.code,
                            e.source()
                        ),
                        Strength::Estimate,
                    )
                };
                Some(
                    Finding::new("HOST-5", format!("{}:{}", e.in_fn, e.code), msg)
                        .span(e.span().label("request refused here"))
                        .fix("count it with `cnt!(..)` before leaving, so the host's mistakes show in the counters")
                        .confidence(e.confidence(s)),
                )
            })
            .collect()
    }
}
