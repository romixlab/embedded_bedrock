//! `FLS-3` RP flash erase or write from core 1, or with core 1 started outside `spawn_core1`

use crate::check::{Check, CheckMeta, Need, Registered};
use crate::finding::Finding;
use crate::model::scan::{self, FnItem, Node};
use crate::model::{Firmware, Loc};
use std::collections::BTreeSet;
use syn::Expr;

pub struct Fls3;

static META: CheckMeta = CheckMeta {
    id: "FLS-3",
    needs: &[Need::Sources],
};

inventory::submit!(Registered(&Fls3));

/// Method names that erase or program through a flash driver (the receiver names `flash`)
const FLASH_OPS: &[&str] = &["blocking_erase", "blocking_write", "erase", "write"];
/// ROM / SDK functions that erase or program flash
const ROM_OPS: &[&str] = &[
    "flash_range_erase",
    "flash_range_program",
    "flash_range_erase_and_program",
];

impl Check for Fls3 {
    fn meta(&self) -> &'static CheckMeta {
        &META
    }

    /// On RP2040 / RP2350 an erase or program stalls XIP for both cores: embassy-rp runs it from RAM on core 0 and
    /// pauses core 1 only when core 1 was started with its `spawn_core1`. Found: a flash operation in code that
    /// core 1 runs (the closure given to `spawn_core1` and the functions it calls by name), and a flash operation
    /// in a firmware that starts core 1 another way (rp-hal's `Multicore`). Not found: DMA reading flash meanwhile.
    fn run(&self, fw: &Firmware) -> Vec<Finding> {
        if !scan::family_in(fw, &["rp2040", "rp235x"]) {
            return Vec::new();
        }
        let fns = scan::functions(fw);
        let ops: Vec<(usize, Loc, String)> = fns
            .iter()
            .enumerate()
            .flat_map(|(i, f)| flash_ops(f).into_iter().map(move |(l, n)| (i, l, n)))
            .collect();
        if ops.is_empty() {
            return Vec::new();
        }
        let mut out = Vec::new();
        // core 1 started by hand
        if let Some(mc) = scan::find_word(fw, |w| w.text == "Multicore") {
            for (i, at, name) in &ops {
                out.push(
                    Finding::new(
                        "FLS-3",
                        format!("{}:{name}", fns[*i].path()),
                        "flash erase / write in a firmware that starts core 1 outside embassy-rp's spawn_core1: nothing pauses core 1, which faults when XIP stalls",
                    )
                    .span(at.span().label(name.clone()))
                    .span(mc.loc.span().label("core 1 started here"))
                    .fix("start core 1 with embassy_rp::multicore::spawn_core1, or run core 1 from RAM and pause it around flash writes yourself"),
                );
            }
        }
        // flash operations on core 1
        let mut core1: BTreeSet<String> = BTreeSet::new();
        let mut starts: Vec<Loc> = Vec::new();
        for f in &fns {
            scan::each(f.block, |n| {
                if let Node::Expr(e @ Expr::Call(c)) = n
                    && scan::call_name(e).is_some_and(|n| n.ends_with("spawn_core1"))
                    && let Some(last) = c.args.last()
                {
                    starts.push(scan::loc(f.file, scan::head_span(e)));
                    names_called(last, &mut core1);
                }
            });
        }
        if starts.is_empty() {
            return out;
        }
        // the functions core 1 reaches by name
        let mut todo: Vec<String> = core1.iter().cloned().collect();
        while let Some(name) = todo.pop() {
            for f in fns.iter().filter(|f| f.name() == name) {
                let block = Expr::Block(syn::ExprBlock {
                    attrs: Vec::new(),
                    label: None,
                    block: f.block.clone(),
                });
                let mut more = BTreeSet::new();
                names_called(&block, &mut more);
                for m in more {
                    if core1.insert(m.clone()) {
                        todo.push(m);
                    }
                }
            }
        }
        for (i, at, name) in &ops {
            if core1.contains(&fns[*i].name()) {
                out.push(
                    Finding::new(
                        "FLS-3",
                        format!("{}:{name}:core1", fns[*i].path()),
                        format!(
                            "flash {name} in {}, which core 1 runs: embassy-rp's flash operations pause core 1 from core 0 and stall XIP, run from core 1 they hang or fault",
                            fns[*i].name()
                        ),
                    )
                    .span(at.span().label(name.clone()))
                    .span(starts[0].span().label("spawn_core1"))
                    .fix("send the write to a task on core 0 (a channel) and do the flash operation there"),
                );
            }
        }
        // operations written right in the closure
        for f in &fns {
            scan::each(f.block, |n| {
                if let Node::Expr(e @ Expr::Call(c)) = n
                    && scan::call_name(e).is_some_and(|n| n.ends_with("spawn_core1"))
                    && let Some(last) = c.args.last()
                {
                    scan::each_in(last, |n| {
                        if let Node::Expr(e) = n
                            && let Some(op) = flash_op(e)
                        {
                            out.push(
                                Finding::new(
                                    "FLS-3",
                                    format!("{}:{op}:core1", f.path()),
                                    format!("flash {op} on core 1: embassy-rp's flash operations must run on core 0"),
                                )
                                .span(scan::loc(f.file, scan::head_span(e)).span())
                                .fix("send the write to a task on core 0 and do the flash operation there"),
                            );
                        }
                    });
                }
            });
        }
        out
    }
}

/// The flash erase / program operations of a function body
fn flash_ops(f: &FnItem<'_>) -> Vec<(Loc, String)> {
    let mut out = Vec::new();
    scan::each(f.block, |n| {
        if let Node::Expr(e) = n
            && let Some(op) = flash_op(e)
        {
            out.push((scan::loc(f.file, scan::head_span(e)), op));
        }
    });
    out
}

fn flash_op(e: &Expr) -> Option<String> {
    match e {
        Expr::MethodCall(m) => {
            let name = m.method.to_string();
            (FLASH_OPS.contains(&name.as_str())
                && scan::text(&*m.receiver)
                    .to_ascii_lowercase()
                    .contains("flash"))
            .then_some(name)
        }
        Expr::Call(_) => {
            let name = scan::call_name(e)?;
            let last = name.rsplit("::").next()?.to_string();
            ROM_OPS.contains(&last.as_str()).then_some(last)
        }
        _ => None,
    }
}

/// The last segments of the functions an expression calls (`core1_task`, `run`), tasks spawned included
fn names_called(e: &Expr, out: &mut BTreeSet<String>) {
    scan::each_in(e, |n| {
        if let Node::Expr(e) = n
            && let Some(name) = scan::call_name(e)
        {
            let last = name
                .trim_start_matches('.')
                .rsplit("::")
                .next()
                .unwrap_or_default();
            out.insert(last.to_string());
        }
    });
}
