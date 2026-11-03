//! `FLT-3` `bedrock_crash::exc_return!()` not the first statement of HardFault

use super::flt;
use crate::check::{Check, CheckMeta, Need, Registered};
use crate::finding::Finding;
use crate::model::Firmware;
use crate::model::scan::{self, Node};
use syn::spanned::Spanned;

pub struct Flt3;

static META: CheckMeta = CheckMeta {
    id: "FLT-3",
    needs: &[Need::Sources],
};

inventory::submit!(Registered(&Flt3));

impl Check for Flt3 {
    fn meta(&self) -> &'static CheckMeta {
        &META
    }

    /// An `exc_return!()` in the HardFault handler that is not in its first statement: anything before it may
    /// call a function and change LR, so the EXC_RETURN it reads is not the fault's.
    fn run(&self, fw: &Firmware) -> Vec<Finding> {
        let fns = scan::functions(fw);
        let Some(h) = flt::handler(&fns, "HardFault") else {
            return Vec::new();
        };
        let mut site = None;
        scan::each(h.block, |n| {
            if let Node::Macro("exc_return", m) = n
                && site.is_none()
            {
                site = Some(m.path.span());
            }
        });
        let Some(site) = site else {
            return Vec::new();
        };
        let first = h.block.stmts.first().map(|s| s.span().start().line);
        if first == Some(site.start().line) {
            return Vec::new();
        }
        vec![
            Finding::new(
                "FLT-3",
                h.path(),
                "`exc_return!()` is not the first statement of HardFault: what runs before it can change LR, and the crash record gets the wrong EXC_RETURN",
            )
            .span(scan::loc(h.file, site).span().label("read here"))
            .fix("make `let exc_return = bedrock_crash::exc_return!();` the handler's first line"),
        ]
    }
}
