//! `EXE-6` State shared between an interrupt executor's task and thread mode behind a thread-only mutex

use crate::check::{Check, CheckMeta, Need, Registered};
use crate::finding::{Confidence, Finding};
use crate::model::Firmware;
use crate::model::context::{Context, Reach};
use crate::model::{Facts, StaticUse};
use std::collections::BTreeMap;

/// The use of a static nearest to a root on one side
fn first_use<'a>(
    facts: &'a Facts,
    side: &'a BTreeMap<usize, Reach>,
    name: &str,
) -> Option<(&'a StaticUse, &'a Reach)> {
    side.iter()
        .filter_map(|(&i, r)| {
            facts.functions[i]
                .statics
                .iter()
                .find(|u| u.name == name)
                .map(|u| (u, r))
        })
        .min_by_key(|(_, r)| r.chain.len())
}

pub struct Exe6;

static META: CheckMeta = CheckMeta {
    id: "EXE-6",
    needs: &[Need::Sources],
};

inventory::submit!(Registered(&Exe6));

impl Check for Exe6 {
    fn meta(&self) -> &'static CheckMeta {
        &META
    }

    /// Statics behind `ThreadModeRawMutex` / `NoopRawMutex` and `static mut`s named in a function that runs in
    /// interrupt context (a task of an interrupt executor, an `#[interrupt]` handler, what they call) and in one that
    /// runs in thread mode: certain when both are the roots themselves, else an estimate (calls are resolved by name)
    fn run(&self, fw: &Firmware) -> Vec<Finding> {
        let facts = &fw.facts;
        let cx = Context::new(facts);
        let mut out = Vec::new();
        for st in &facts.statics {
            let kind = match st.raw_mutex() {
                Some(m @ ("ThreadModeRawMutex" | "NoopRawMutex")) => m.to_string(),
                Some(_) => continue,
                None if st.mutable && !st.is_interrupt_executor() => "static mut".to_string(),
                None => continue,
            };
            let uses = |side| first_use(facts, side, &st.name);
            let (Some((iu, ir)), Some((tu, tr))) = (uses(&cx.interrupt), uses(&cx.thread)) else {
                continue;
            };
            let confidence = if ir.direct() && tr.direct() {
                Confidence::Certain
            } else {
                Confidence::Estimate
            };
            out.push(
                Finding::new(
                    "EXE-6",
                    st.path.clone(),
                    format!(
                        "{} ({kind}) is used in interrupt context ({}) and in thread mode ({}): {}",
                        st.name,
                        cx.chain_text(ir),
                        cx.chain_text(tr),
                        if kind == "static mut" {
                            "the interrupt can preempt thread mode in the middle of an access"
                        } else {
                            "that mutex doesn't keep the interrupt out"
                        }
                    ),
                )
                .span(st.loc.span().label(format!("{} ({kind})", st.name)))
                .span(iu.loc.span().label("used in interrupt context"))
                .span(tu.loc.span().label("used in thread mode"))
                .fix("put it behind `CriticalSectionRawMutex` (or an atomic), or keep it to one context")
                .confidence(confidence),
            );
        }
        out
    }
}
