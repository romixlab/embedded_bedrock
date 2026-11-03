//! `STK-5` Big state built by value and moved into a static

use crate::check::{Check, CheckMeta, Need, Registered};
use crate::config::Config;
use crate::finding::{Confidence, Finding, Span};
use crate::model::Firmware;
use crate::model::elf::bytes;

pub struct Stk5;

static META: CheckMeta = CheckMeta {
    id: "STK-5",
    needs: &[Need::Sources, Need::Elf, Need::Thumb],
};

inventory::submit!(Registered(&Stk5));

impl Check for Stk5 {
    fn meta(&self) -> &'static CheckMeta {
        &META
    }

    /// The ELF part: a source function that calls `.init(..)` on a `StaticCell` static, whose code in the ELF (the
    /// function, its async body or the embassy task it is) has a frame of `[limits] move-frame` bytes or more. An
    /// estimate: the frame may hold other things too.
    fn run_with(&self, fw: &Firmware, cfg: &Config) -> Vec<Finding> {
        let Some(elf) = &fw.elf else {
            return Vec::new();
        };
        let cells: Vec<&str> = fw
            .facts
            .statics
            .iter()
            .filter(|s| s.ty.contains("StaticCell"))
            .map(|s| s.name.as_str())
            .collect();
        if cells.is_empty() {
            return Vec::new();
        }
        let mut out = Vec::new();
        for f in &fw.facts.functions {
            let Some(text) = fw.text_of(&f.loc.file) else {
                continue;
            };
            let lines: Vec<&str> = text.lines().collect();
            let Some((call, cell)) = f.calls.iter().filter(|c| c.name == ".init").find_map(|c| {
                let line = lines.get(c.loc.line.saturating_sub(1) as usize)?;
                let cell = cells
                    .iter()
                    .find(|n| line.contains(&format!("{n}.init(")))?;
                Some((c, *cell))
            }) else {
                continue;
            };
            // the function itself, its async body, or the embassy task wrapping it
            let (module, name) = f.path.rsplit_once("::").unwrap_or(("", f.path.as_str()));
            let task = format!("{module}::__{name}_task");
            let main_task = format!("{module}::____embassy_{name}_task");
            let frame = elf
                .code
                .iter()
                .filter(|c| {
                    let n = &elf.symbols[c.symbol].name;
                    *n == f.path
                        || n.starts_with(&format!("{}::{{closure", f.path))
                        || n.contains(&task)
                        || n.contains(&main_task)
                })
                .filter_map(|c| c.frame)
                .max();
            let Some(frame) = frame.filter(|&fr| fr >= cfg.limits.move_frame) else {
                continue;
            };
            out.push(
                Finding::new(
                    "STK-5",
                    format!("{}:{cell}.init", f.path),
                    format!(
                        "`{}` moves a value into the static `{cell}` with `.init(..)`; its frame in the ELF is {}: the value is built on the stack first",
                        f.path,
                        bytes(frame as u64)
                    ),
                )
                .span(Span::at(&call.loc.file, call.loc.line, call.loc.column))
                .fix("build it in place: a `const fn new` and a plain `static` / `ConstStaticCell`, or `uninit()` and initialize the fields through the `MaybeUninit`")
                .confidence(Confidence::Estimate),
            );
        }
        out
    }
}
