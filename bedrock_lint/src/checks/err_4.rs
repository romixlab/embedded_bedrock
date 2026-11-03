//! `ERR-4` FDCAN edge filtering (`CCCR.EFBI`) on, or dedicated Tx buffers beside the Tx FIFO

use crate::check::{Check, CheckMeta, Need, Registered};
use crate::finding::Finding;
use crate::model::Firmware;
use crate::model::scan::{self, Node};
use syn::Expr;

pub struct Err4;

static META: CheckMeta = CheckMeta {
    id: "ERR-4",
    needs: &[Need::Sources],
};

inventory::submit!(Registered(&Err4));

/// Setters that turn edge filtering on: the PAC field's, embassy-stm32's and mcan's
const EDGE: &[&str] = &["set_efbi", "set_edge_filtering", "edge_filtering"];

impl Check for Err4 {
    fn meta(&self) -> &'static CheckMeta {
        &META
    }

    /// On G0 and H7 FDCAN, with a literal `true`: `set_efbi`, `set_edge_filtering`, `efbi: true`; and a
    /// number of dedicated Tx buffers (`set_ndtb(n)`, n > 0) in a function that doesn't put the Tx queue in queue
    /// mode (`set_tfqm(true)`). ES0548 Rev 4 2.15.1 / ES0491 Rev 9 2.22.1 "Desynchronization under specific
    /// condition with edge filtering enabled", workaround "Disable edge filtering or wait for frame retransmission";
    /// ES0548 2.15.2 / ES0491 2.22.2 "Tx FIFO messages inverted under specific buffer usage and priority setting",
    /// when "FDCAN uses both a dedicated Tx buffer and a Tx FIFO (the TFQM bit of the FDCAN_TXBC register is
    /// cleared)", workaround "Use only a Tx FIFO" (status A on every revision the sheets list; quoted in
    /// firmware_template_skill/references/errata.md). A value decided at run time (the host's) isn't judged.
    fn run(&self, fw: &Firmware) -> Vec<Finding> {
        if !scan::family_in(fw, &["stm32g0", "stm32h7"]) {
            return Vec::new();
        }
        let consts = scan::consts(fw);
        let mut out = Vec::new();
        for f in scan::functions(fw) {
            let calls = scan::method_calls(&f);
            for c in &calls {
                if EDGE.contains(&c.name.as_str())
                    && c.args.len() == 1
                    && scan::bool_value(&c.args[0]) == Some(true)
                {
                    out.push(edge(&f.path(), c.loc.span()));
                }
            }
            scan::each(f.block, |n| {
                if let Node::Expr(Expr::Struct(s)) = n {
                    for fv in &s.fields {
                        let m = scan::text(&fv.member);
                        // the register field only: `edge_filtering: true` in bedrock_can's Capabilities says it
                        // is supported, not that it is on
                        if m == "efbi" && scan::bool_value(&fv.expr) == Some(true) {
                            out.push(edge(
                                &f.path(),
                                scan::loc(f.file, syn::spanned::Spanned::span(fv)).span(),
                            ));
                        }
                    }
                }
            });
            let queue_mode = calls.iter().any(|c| {
                c.name == "set_tfqm" && c.args.first().and_then(scan::bool_value) == Some(true)
            });
            for c in calls.iter().filter(|c| c.name == "set_ndtb") {
                let n = c.args.first().and_then(|a| scan::int_value(a, &consts));
                if n.is_some_and(|n| n > 0) && !queue_mode {
                    out.push(
                        Finding::new(
                            "ERR-4",
                            format!("{}:ndtb", f.path()),
                            "dedicated Tx buffers beside the Tx FIFO: ES0548 2.15.2 / ES0491 2.22.2 \"Tx FIFO messages inverted under specific buffer usage and priority setting\"",
                        )
                        .span(c.loc.span())
                        .fix("use only the Tx FIFO (NDTB = 0), the sheets' workaround \"Use only a Tx FIFO\", or the Tx queue (TFQM = 1)"),
                    );
                }
            }
        }
        out
    }
}

fn edge(path: &str, span: crate::Span) -> Finding {
    Finding::new(
        "ERR-4",
        format!("{path}:efbi"),
        "FDCAN edge filtering on: ES0548 2.15.1 / ES0491 2.22.1 \"Desynchronization under specific condition with edge filtering enabled\", the first bit of a frame can be received wrong",
    )
    .span(span)
    .fix("leave CCCR.EFBI off (the sheets' workaround: \"Disable edge filtering or wait for frame retransmission\")")
}
