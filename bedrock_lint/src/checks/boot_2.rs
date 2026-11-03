//! `BOOT-2` H7 supply config written blind

use crate::check::{Check, CheckMeta, Need, Registered};
use crate::finding::Finding;
use crate::model::Firmware;
use crate::model::scan::{self, Node};
use syn::Expr;

pub struct Boot2;

static META: CheckMeta = CheckMeta {
    id: "BOOT-2",
    needs: &[Need::Sources],
};

inventory::submit!(Registered(&Boot2));

impl Check for Boot2 {
    fn meta(&self) -> &'static CheckMeta {
        &META
    }

    /// On an H7, a constant `supply_config = SupplyConfig::..` (or a write of PWR_CR3) in a firmware that never reads
    /// PWR_CR3 back: ROM bootloaders before V9.3 lock the register until a power cycle (AN2606, bedrock_rom_boot
    /// `h7`), the write is ignored and embassy waits for a supply state that never comes. A value from a function
    /// (vb135's `supply_config()`, which reads the register first) is fine.
    fn run(&self, fw: &Firmware) -> Vec<Finding> {
        if !scan::family_in(fw, &["stm32h7"]) {
            return Vec::new();
        }
        let words = scan::words(fw);
        let reads_back = words.iter().any(|w| {
            (w.has("pwr") && w.has("cr3")) || w.text == "check_supply" || w.text == "entered"
        }) || scan::functions(fw).iter().any(|f| {
            scan::method_calls(f)
                .iter()
                .any(|c| c.name == "read" && c.receiver.ends_with("cr3()"))
        });
        if reads_back {
            return Vec::new();
        }
        let mut out = Vec::new();
        for f in scan::functions(fw) {
            let mut writes = Vec::new();
            scan::each(f.block, |n| match n {
                Node::Expr(Expr::Assign(a)) => {
                    if let Expr::Field(field) = &*a.left
                        && scan::text(&field.member) == "supply_config"
                        && is_constant(&a.right)
                    {
                        writes.push((
                            scan::loc(f.file, syn::spanned::Spanned::span(a)),
                            scan::text(&*a.right),
                        ));
                    }
                }
                Node::Expr(Expr::Struct(s)) => {
                    for fv in &s.fields {
                        if scan::text(&fv.member) == "supply_config" && is_constant(&fv.expr) {
                            writes.push((
                                scan::loc(f.file, syn::spanned::Spanned::span(fv)),
                                scan::text(&fv.expr),
                            ));
                        }
                    }
                }
                Node::Expr(Expr::MethodCall(m))
                    if matches!(m.method.to_string().as_str(), "modify" | "write")
                        && scan::text(&*m.receiver).ends_with("cr3()")
                        && scan::text(&*m.receiver)
                            .to_ascii_lowercase()
                            .contains("pwr") =>
                {
                    writes.push((scan::loc(f.file, m.method.span()), "PWR_CR3".into()));
                }
                _ => {}
            });
            for (loc, value) in writes {
                out.push(
                    Finding::new(
                        "BOOT-2",
                        f.path(),
                        format!(
                            "the supply configuration is written blind ({value}): a ROM bootloader before V9.3 leaves PWR_CR3 locked until a power cycle, the write is ignored and the firmware runs on a supply it didn't choose"
                        ),
                    )
                    .span(loc.span())
                    .fix("read PWR_CR3 first (bedrock_rom_boot::entered().pwr_cr3), give embassy what it holds when it is locked, and count the mismatch"),
                );
            }
        }
        out
    }
}

/// `SupplyConfig::DirectSMPS`, `SupplyConfig::SMPSLDO(..)`: a value fixed in the source
fn is_constant(e: &Expr) -> bool {
    match e {
        Expr::Path(p) => scan::text(&p.path).contains("SupplyConfig::"),
        Expr::Call(c) => {
            matches!(&*c.func, Expr::Path(p) if scan::text(&p.path).contains("SupplyConfig::"))
        }
        Expr::Paren(p) => is_constant(&p.expr),
        _ => false,
    }
}
