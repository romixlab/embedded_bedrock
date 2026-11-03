//! `EXT-4` Read-modify-write of an external chip's registers over the bus

use crate::check::{Check, CheckMeta, Need, Registered};
use crate::finding::Finding;
use crate::model::Firmware;
use crate::model::scan;
use syn::Expr;

pub struct Ext4;

static META: CheckMeta = CheckMeta {
    id: "EXT-4",
    needs: &[Need::Sources],
};

inventory::submit!(Registered(&Ext4));

impl Check for Ext4 {
    fn meta(&self) -> &'static CheckMeta {
        &META
    }

    /// In one function, a register read over I2C / SPI (`write_read(addr, &[REG], buf)`, `read_register(REG)`,
    /// `read_reg(..)`) and later a write of the same register (`write(addr, &[REG, ..])`, `write_register(REG, ..)`):
    /// a read-modify-write of an external chip, which loses bits the chip changed between the two and re-reads
    /// clear-on-read flags. Registers are compared as written (`REG_CTRL`, `0x1A`). Clear-on-read registers read
    /// twice need the chip's register map and are not found here.
    fn run(&self, fw: &Firmware) -> Vec<Finding> {
        let mut out = Vec::new();
        for f in scan::functions(fw) {
            let calls = scan::method_calls(&f);
            let reads: Vec<(String, &scan::MethodCall)> = calls
                .iter()
                .filter_map(|c| read_reg(c).map(|r| (r, c)))
                .collect();
            for c in &calls {
                let Some(reg) = write_reg(c) else { continue };
                if let Some((_, r)) = reads.iter().find(|(rr, r)| *rr == reg && r.loc < c.loc) {
                    out.push(
                        Finding::new(
                            "EXT-4",
                            format!("{}:{reg}", f.path()),
                            format!(
                                "register {reg} of an external chip is read, changed and written back: bits the chip changed meanwhile are lost, clear-on-read flags are read again"
                            ),
                        )
                        .span(c.loc.span().label("written back"))
                        .span(r.loc.span().label("read"))
                        .fix("keep a shadow copy of the register in the driver and write from it; read clear-on-read registers once"),
                    );
                }
            }
        }
        out
    }
}

/// The register a bus read names
fn read_reg(c: &scan::MethodCall) -> Option<String> {
    match c.name.as_str() {
        // write_read(addr, &[REG], buf)
        "write_read" if c.args.len() == 3 => first_byte(&c.args[1]),
        "read_register" | "read_reg" | "read_u8" | "read_byte_data" => c
            .args
            .iter()
            .rev()
            .nth(usize::from(c.args.len() > 1))
            .map(reg_text),
        _ => None,
    }
}

/// The register a bus write names
fn write_reg(c: &scan::MethodCall) -> Option<String> {
    match c.name.as_str() {
        // write(addr, &[REG, value, ..])
        "write" if c.args.len() == 2 => {
            let Expr::Reference(r) = &c.args[1] else {
                return None;
            };
            let Expr::Array(a) = &*r.expr else {
                return None;
            };
            (a.elems.len() >= 2).then(|| reg_text(&a.elems[0]))
        }
        "write_register" | "write_reg" | "write_u8" | "write_byte_data" if c.args.len() >= 2 => {
            Some(reg_text(&c.args[c.args.len() - 2]))
        }
        _ => None,
    }
}

/// `&[REG]` → `REG`
fn first_byte(e: &Expr) -> Option<String> {
    let e = match e {
        Expr::Reference(r) => &*r.expr,
        e => e,
    };
    match e {
        Expr::Array(a) if a.elems.len() == 1 => Some(reg_text(&a.elems[0])),
        _ => None,
    }
}

/// A register as written, casts dropped (`Reg::Ctrl as u8` → `Reg::Ctrl`)
fn reg_text(e: &Expr) -> String {
    match e {
        Expr::Cast(c) => reg_text(&c.expr),
        Expr::Paren(p) => reg_text(&p.expr),
        e => scan::text(e),
    }
}
