//! `HOST-3` Unknown enum value from a newer host mapped to a default instead of refused

use super::host::{self, EventKind, Strength};
use crate::check::{Check, CheckMeta, Need, Registered};
use crate::finding::Finding;
use crate::model::Firmware;

pub struct Host3;

static META: CheckMeta = CheckMeta {
    id: "HOST-3",
    needs: &[Need::Sources],
};

inventory::submit!(Registered(&Host3));

impl Check for Host3 {
    fn meta(&self) -> &'static CheckMeta {
        &META
    }

    /// A `match` on a host value whose other arms are literals or paths, with a catch-all arm (`_`, or a name it
    /// does not use) that does not refuse (no `Err`, `Unimplemented`, `Invalid`, `Unknown`, `return`); a
    /// `try_from(..)` / `from_repr(..)` / `try_into()` of a host value with `unwrap_or*`. An estimate: the value
    /// may be a number where a default is right.
    fn run(&self, fw: &Firmware) -> Vec<Finding> {
        host::analyze(fw)
            .into_iter()
            .filter(|e| matches!(e.kind, EventKind::Default))
            .map(|e| {
                Finding::new(
                    "HOST-3",
                    format!("{}:{}", e.in_fn, e.code),
                    format!(
                        "a value the host chose falls back to a default at `{}`: a newer host's new variant is served as something else ({})",
                        e.code,
                        e.source()
                    ),
                )
                .span(e.span().label("catch-all"))
                .fix("refuse values this firmware does not know with an error the host can show")
                .confidence(e.confidence(Strength::Estimate))
            })
            .collect()
    }
}
