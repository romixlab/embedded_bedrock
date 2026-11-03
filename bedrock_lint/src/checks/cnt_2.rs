//! `CNT-2` `&mut` to a counter buffer that an interrupt can update, unaligned u64 access

use crate::check::{Check, CheckMeta, Need, Registered};
use crate::finding::Finding;
use crate::model::Firmware;
use crate::model::scan::{self, Node};
use syn::Expr;

pub struct Cnt2;

static META: CheckMeta = CheckMeta {
    id: "CNT-2",
    needs: &[Need::Sources],
};

inventory::submit!(Registered(&Cnt2));

/// What names the counter buffers: the `cnt` crate's statics and accessors, the linker symbols
const BUFFERS: &[&str] = &[
    "_CNT_RAM_BUFFER",
    "_CNT_BKP_BUFFER",
    "counters_ram_buffer",
    "counters_bkp_buffer",
    "__RAM_COUNTERS",
    "__BKP_COUNTERS",
];

impl Check for Cnt2 {
    fn meta(&self) -> &'static CheckMeta {
        &META
    }

    /// On an expression that names a counter buffer: `&mut ..`, `as_mut_ptr()`, `from_raw_parts_mut(..)`, a cast to
    /// `*mut` / `*const u64`, `read_unaligned` / `write_unaligned`. The `cnt` crate (0.4 and later) updates the
    /// words atomically; a `&mut` beside it is undefined behaviour once an interrupt counts, and a u64 read through
    /// one pointer is unaligned and tears on a carry.
    fn run(&self, fw: &Firmware) -> Vec<Finding> {
        let mut out = Vec::new();
        for f in scan::functions(fw) {
            let path = f.path();
            scan::each(f.block, |n| {
                let Node::Expr(e) = n else { return };
                let (what, inner): (&str, &Expr) = match e {
                    Expr::Reference(r) if r.mutability.is_some() => ("&mut", &r.expr),
                    Expr::MethodCall(m)
                        if matches!(
                            m.method.to_string().as_str(),
                            "as_mut_ptr" | "read_unaligned" | "write_unaligned"
                        ) =>
                    {
                        ("pointer access", &m.receiver)
                    }
                    Expr::Call(_)
                        if scan::call_name(e).is_some_and(|n| {
                            n.ends_with("from_raw_parts_mut")
                                || n.ends_with("read_unaligned")
                                || n.ends_with("write_unaligned")
                        }) =>
                    {
                        ("pointer access", e)
                    }
                    Expr::Cast(c) => {
                        let ty = scan::text(&c.ty).replace(' ', "");
                        if ty.starts_with("*mut") || ty.contains("u64") {
                            ("pointer cast", &c.expr)
                        } else {
                            return;
                        }
                    }
                    _ => return,
                };
                let t = scan::text(inner);
                let Some(buf) = BUFFERS.iter().find(|b| t.contains(*b)) else {
                    return;
                };
                out.push(
                    Finding::new(
                        "CNT-2",
                        format!("{path}:{what}:{buf}"),
                        format!(
                            "{what} on the counter buffer `{buf}` in `{path}`: an interrupt that counts meanwhile makes it undefined behaviour, and a u64 counter read through one pointer is unaligned and tears"
                        ),
                    )
                    .span(scan::loc(f.file, scan::head_span(e)).span().label(format!("{what} on {buf}")))
                    .fix("read the buffer as `&[AtomicU32]` (`cnt::counters_ram_buffer()`), a u64 counter as two words with the high word read twice (cnt_core does), and count with `cnt!` only"),
                );
            });
        }
        out
    }
}
