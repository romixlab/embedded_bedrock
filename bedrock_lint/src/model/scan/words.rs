//! What the source names, for the checks that ask "does the firmware do X anywhere" (a PVD, a clock security
//! system, a backup-domain reset) or "in what order does this function touch registers": every identifier of the
//! source with its place ([`words`], macro bodies included as tokens), identifiers split into lower-case words
//! ([`segments`]: `set_csson` → set, csson; `RtcClockSource` → rtc, clock, source), the method calls of a body in
//! source order ([`method_calls`]), integer constants and literals ([`int_value`]), and the comment block above a
//! line ([`comment_above`]).

use super::{FnItem, is_test, loc, text};
use crate::model::Firmware;
use crate::model::source::{Loc, SourceFile};
use proc_macro2::{TokenStream, TokenTree};
use std::collections::BTreeMap;
use syn::visit::{self, Visit};
use syn::{Expr, Lit};

/// An identifier of the source and where it is
#[derive(Clone, Debug)]
pub struct Word {
    pub text: String,
    pub loc: Loc,
}

impl Word {
    /// Whether one of its [`segments`] is `seg`
    pub fn has(&self, seg: &str) -> bool {
        has_segment(&self.text, seg)
    }
}

/// Every identifier of the firmware's source (paths, methods, fields, macro bodies), test code left out
pub fn words(fw: &Firmware) -> Vec<Word> {
    let mut out = Vec::new();
    for f in &fw.sources {
        let mut v = Words {
            file: f,
            out: &mut out,
        };
        v.visit_file(&f.ast);
    }
    out
}

/// The identifiers of one function body
pub fn words_in(f: &FnItem<'_>) -> Vec<Word> {
    let mut out = Vec::new();
    let mut v = Words {
        file: f.file,
        out: &mut out,
    };
    v.visit_block(f.block);
    out
}

/// The first identifier of the source that `pred` accepts
pub fn find_word(fw: &Firmware, pred: impl Fn(&Word) -> bool) -> Option<Word> {
    words(fw).into_iter().find(|w| pred(w))
}

/// Whether any identifier has the segment `seg` (`hsi48`, `css`)
pub fn mentions(words: &[Word], seg: &str) -> bool {
    words.iter().any(|w| w.has(seg))
}

struct Words<'a, 'o> {
    file: &'a SourceFile,
    out: &'o mut Vec<Word>,
}

impl Words<'_, '_> {
    fn stream(&mut self, ts: TokenStream) {
        for t in ts {
            match t {
                TokenTree::Ident(i) => self.out.push(Word {
                    text: i.to_string(),
                    loc: loc(self.file, i.span()),
                }),
                TokenTree::Group(g) => self.stream(g.stream()),
                _ => {}
            }
        }
    }
}

impl<'ast> Visit<'ast> for Words<'_, '_> {
    fn visit_ident(&mut self, i: &'ast proc_macro2::Ident) {
        self.out.push(Word {
            text: i.to_string(),
            loc: loc(self.file, i.span()),
        });
    }

    fn visit_macro(&mut self, m: &'ast syn::Macro) {
        visit::visit_path(self, &m.path);
        self.stream(m.tokens.clone());
    }

    // attributes are not code (doc comments especially)
    fn visit_attribute(&mut self, _: &'ast syn::Attribute) {}

    fn visit_item_fn(&mut self, f: &'ast syn::ItemFn) {
        if !is_test(&f.attrs) {
            visit::visit_item_fn(self, f);
        }
    }

    fn visit_item_mod(&mut self, m: &'ast syn::ItemMod) {
        if !is_test(&m.attrs) {
            visit::visit_item_mod(self, m);
        }
    }

    fn visit_item_impl(&mut self, i: &'ast syn::ItemImpl) {
        if !is_test(&i.attrs) {
            visit::visit_item_impl(self, i);
        }
    }

    fn visit_impl_item_fn(&mut self, f: &'ast syn::ImplItemFn) {
        if !is_test(&f.attrs) {
            visit::visit_impl_item_fn(self, f);
        }
    }
}

/// An identifier split into lower-case words at `_` and at case changes: `set_csson` → set, csson;
/// `RtcClockSource` → rtc, clock, source; `HSI48` → hsi48; `LsConfig` → ls, config
pub fn segments(name: &str) -> Vec<String> {
    let mut out = Vec::new();
    for part in name.split('_').filter(|p| !p.is_empty()) {
        let chars: Vec<char> = part.chars().collect();
        let mut cur = String::new();
        for (i, &c) in chars.iter().enumerate() {
            let prev = i.checked_sub(1).map(|j| chars[j]);
            let next = chars.get(i + 1).copied();
            // a new word at a lower → upper change, and at the last capital of a run before a lower (`HSEConfig`)
            let boundary = c.is_uppercase()
                && prev.is_some_and(|p| {
                    p.is_lowercase() || (p.is_uppercase() && next.is_some_and(char::is_lowercase))
                });
            if boundary && !cur.is_empty() {
                out.push(std::mem::take(&mut cur).to_lowercase());
            }
            cur.push(c);
        }
        if !cur.is_empty() {
            out.push(cur.to_lowercase());
        }
    }
    out
}

/// Whether `name` has the segment `seg` (lower case)
pub fn has_segment(name: &str, seg: &str) -> bool {
    segments(name).iter().any(|s| s == seg)
}

/// A method call of a body, in source order
#[derive(Clone)]
pub struct MethodCall {
    /// The method (`set_addis`)
    pub name: String,
    /// The receiver as written (`r.cr()`, `w`)
    pub receiver: String,
    pub args: Vec<Expr>,
    pub loc: Loc,
}

impl MethodCall {
    /// The single argument as written
    pub fn arg_text(&self) -> String {
        self.args.iter().map(text).collect::<Vec<_>>().join(", ")
    }
}

/// The method calls of a body, macro bodies included, sorted by place (a closure's calls after the call that takes
/// it: `r.cr().modify(|w| w.set_aden(true))` gives `cr`, `modify`, `set_aden`)
pub fn method_calls(f: &FnItem<'_>) -> Vec<MethodCall> {
    let mut out = Vec::new();
    super::each(f.block, |n| {
        if let super::Node::Expr(Expr::MethodCall(m)) = n {
            out.push(MethodCall {
                name: m.method.to_string(),
                receiver: text(&*m.receiver),
                args: m.args.iter().cloned().collect(),
                loc: loc(f.file, m.method.span()),
            });
        }
    });
    out.sort_by(|a, b| a.loc.cmp(&b.loc));
    out
}

/// The call names of a body (`delay_us`, `.block_for`, `Timer::after_micros`), each with its place
pub fn calls_in(f: &FnItem<'_>) -> Vec<(String, Loc)> {
    let mut out = Vec::new();
    super::each(f.block, |n| match n {
        super::Node::Expr(e) => {
            if let Some(name) = super::call_name(e) {
                out.push((name, loc(f.file, super::head_span(e))));
            }
        }
        super::Node::Macro(name, m) => out.push((
            format!("{name}!"),
            loc(f.file, m.path.segments[0].ident.span()),
        )),
    });
    out.sort_by(|a, b| a.1.cmp(&b.1));
    out
}

/// `const NAME: T = <expr>;` items of the source (modules and impls included), by name
pub fn consts(fw: &Firmware) -> BTreeMap<String, Expr> {
    struct V<'m>(&'m mut BTreeMap<String, Expr>);
    impl<'ast> Visit<'ast> for V<'_> {
        fn visit_item_const(&mut self, c: &'ast syn::ItemConst) {
            self.0.insert(c.ident.to_string(), (*c.expr).clone());
        }
        fn visit_impl_item_const(&mut self, c: &'ast syn::ImplItemConst) {
            self.0.insert(c.ident.to_string(), c.expr.clone());
        }
    }
    let mut out = BTreeMap::new();
    for f in &fw.sources {
        V(&mut out).visit_file(&f.ast);
    }
    out
}

/// The value of an integer literal (`512`, `0x200`, `1_000u32`), through parens, casts and a `const` of `consts`
pub fn int_value(e: &Expr, consts: &BTreeMap<String, Expr>) -> Option<u128> {
    match e {
        Expr::Lit(l) => match &l.lit {
            Lit::Int(i) => i.base10_parse().ok(),
            _ => None,
        },
        Expr::Paren(p) => int_value(&p.expr, consts),
        Expr::Cast(c) => int_value(&c.expr, consts),
        Expr::Path(p) if p.qself.is_none() => {
            let name = p.path.segments.last()?.ident.to_string();
            int_value(consts.get(&name)?, &BTreeMap::new())
        }
        _ => None,
    }
}

/// Whether an expression is the literal `true` / `false`
pub fn bool_value(e: &Expr) -> Option<bool> {
    match e {
        Expr::Lit(l) => match &l.lit {
            Lit::Bool(b) => Some(b.value),
            _ => None,
        },
        Expr::Paren(p) => bool_value(&p.expr),
        _ => None,
    }
}

/// The comment of a line: its own trailing `//` comment and the block of `//` lines right above it, joined
pub fn comment_above(file: &SourceFile, line: u32) -> String {
    let lines: Vec<&str> = file.text.lines().collect();
    let Some(idx) = (line as usize).checked_sub(1) else {
        return String::new();
    };
    let mut parts = Vec::new();
    if let Some(l) = lines.get(idx)
        && let Some(p) = l.find("//")
    {
        parts.push(l[p + 2..].trim().to_string());
    }
    let mut i = idx;
    while i > 0 {
        i -= 1;
        let t = lines[i].trim_start();
        if let Some(c) = t.strip_prefix("//") {
            parts.push(c.trim_start_matches(['/', '!']).trim().to_string());
        } else {
            break;
        }
    }
    parts.reverse();
    parts.join(" ")
}

/// Where to report something the firmware lacks: its entry point (`#[embassy_executor::main]`, `#[entry]`), else
/// the first line of its first source file
pub fn entry_loc(fw: &Firmware) -> Loc {
    if let Some(e) = fw.facts.entries.first() {
        return e.loc.clone();
    }
    Loc {
        file: fw
            .sources
            .first()
            .map_or("src/main.rs".into(), |f| f.rel.clone()),
        line: 1,
        column: 1,
    }
}

/// Whether the chip family is one of `families` (`stm32g0`, `rp235x`); `false` when it is not known
pub fn family_in(fw: &Firmware, families: &[&str]) -> bool {
    super::family(fw).is_some_and(|f| families.contains(&f.as_str()))
}

/// Whether the firmware runs on an STM32
pub fn is_stm32(fw: &Firmware) -> bool {
    super::family(fw).is_some_and(|f| f.starts_with("stm32"))
}

/// Whether the crate depends on `package` (its real name, `embassy-stm32`)
pub fn depends_on(fw: &Firmware, package: &str) -> bool {
    fw.dependencies().iter().any(|d| d.package() == package)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_identifiers_into_words() {
        assert_eq!(segments("set_csson"), ["set", "csson"]);
        assert_eq!(segments("RtcClockSource"), ["rtc", "clock", "source"]);
        assert_eq!(segments("HSI48"), ["hsi48"]);
        assert_eq!(segments("HSEConfig"), ["hse", "config"]);
        assert_eq!(segments("bkp_cnt"), ["bkp", "cnt"]);
        assert!(!has_segment("false_lse_x", "ls"));
        assert!(has_segment("LseConfig", "lse"));
    }

    #[test]
    fn reads_integers_and_comments() {
        let e: Expr = syn::parse_str("(0x200u16)").unwrap();
        assert_eq!(int_value(&e, &BTreeMap::new()), Some(512));
        let mut c = BTreeMap::new();
        c.insert("MPS".to_string(), syn::parse_str::<Expr>("64").unwrap());
        let e: Expr = syn::parse_str("crate::usb::MPS as u16").unwrap();
        assert_eq!(int_value(&e, &c), Some(64));
        let file = SourceFile {
            rel: "src/main.rs".into(),
            krate: "fw".into(),
            module: String::new(),
            text: "fn a() {\n    // ES0548 2.2.10\n    // \"Prefetch failure\"\n    poke(); // A, Z\n}\n".into(),
            ast: syn::parse_file("fn a() {}").unwrap(),
        };
        assert_eq!(
            comment_above(&file, 4),
            "ES0548 2.2.10 \"Prefetch failure\" A, Z"
        );
    }
}
