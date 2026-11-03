//! `HOST-1` Panic reachable from host input

use super::host::{self, EventKind};
use crate::check::{Check, CheckMeta, Need, Registered};
use crate::finding::Finding;
use crate::model::Firmware;

pub struct Host1;

static META: CheckMeta = CheckMeta {
    id: "HOST-1",
    needs: &[Need::Sources],
};

inventory::submit!(Registered(&Host1));

impl Check for Host1 {
    fn meta(&self) -> &'static CheckMeta {
        &META
    }

    /// Each panic a host value reaches in a WireWeaver handler or a function it hands the value to (see
    /// [`host`]): `unwrap` / `expect` on a result the value decides, indexing with it or into it, division by it,
    /// `assert!` on it, `todo!` / `unimplemented!` in a handler, `panic!` / `unreachable!` in a `match` on it. Overflow
    /// of `+ - * << >>` on it is an estimate with `overflow-checks` on in release, else a candidate (it panics in dev
    /// builds). A place whose names the function also compares or bounds is a candidate: it may be guarded.
    fn run(&self, fw: &Firmware) -> Vec<Finding> {
        let mut out = Vec::new();
        for e in host::analyze(fw) {
            let EventKind::Panic { what, strength } = e.kind else {
                continue;
            };
            let (msg, fix) = match what {
                "unwrap" => (
                    format!(
                        "`{}` panics when the host sends a value it fails on ({})",
                        e.code,
                        e.source()
                    ),
                    "match the result and answer the host with an error",
                ),
                "index" => (
                    format!(
                        "`{}` panics on an index or a length the host chose ({})",
                        e.code,
                        e.source()
                    ),
                    "use `.get(..)` and answer the host with an error when it is out of range",
                ),
                "divide" => (
                    format!("`{}` panics when the host sends 0 ({})", e.code, e.source()),
                    "refuse 0 with an error, or use `checked_div`",
                ),
                "overflow" => (
                    format!(
                        "`{}` overflows on a large value from the host, a panic with overflow checks on ({})",
                        e.code,
                        e.source()
                    ),
                    "range-check the value, or use `checked_*` / `saturating_*` and refuse what does not fit",
                ),
                "assert!" => (
                    format!(
                        "`{}` panics on a value from the host ({})",
                        e.code,
                        e.source()
                    ),
                    "check the value and answer the host with an error",
                ),
                _ => (
                    format!("`{what}` in reach of the host ({})", e.source()),
                    "answer the host with an error (`Unimplemented`, an error variant) instead of panicking",
                ),
            };
            out.push(
                Finding::new("HOST-1", format!("{}:{what}:{}", e.in_fn, e.code), msg)
                    .span(e.span().label(format!("{what} on host input")))
                    .fix(fix)
                    .confidence(e.confidence(strength)),
            );
        }
        out
    }
}
