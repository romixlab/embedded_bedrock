//! The linked ELF: sections with their load address, LOAD segments, symbols (demangled), and for Arm Thumb code
//! the frame each function's prologue sets up and the functions it calls ([`thumb`](super::thumb)).
//!
//! Read with `object`'s low-level ELF parser; only 32-bit ELFs (every chip the template knows).

use super::Region;
use super::thumb;
use crate::finding::Span;
use object::elf;
use object::read::elf::{FileHeader, ProgramHeader, SectionHeader, Sym};
use serde::Serialize;
use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};

/// The CPU the ELF is for
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Machine {
    Arm,
    RiscV,
    Xtensa,
    Other(u16),
}

/// An output section
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Section {
    pub name: String,
    /// Where it runs (VMA)
    pub addr: u64,
    pub size: u64,
    /// Where it is stored (LMA), from the LOAD segment that holds it; the VMA when no segment does
    pub load: u64,
    /// Takes memory at run time (`SHF_ALLOC`)
    pub alloc: bool,
    pub write: bool,
    pub exec: bool,
    /// No bytes in the file (`.bss`, `NOLOAD`)
    pub nobits: bool,
}

impl Section {
    pub fn end(&self) -> u64 {
        self.addr + self.size
    }

    /// Copied from flash at startup: stored somewhere else than it runs
    pub fn is_init_data(&self) -> bool {
        self.alloc && !self.nobits && self.write && self.load != self.addr
    }
}

/// A LOAD segment
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Segment {
    /// Its index among the program headers
    pub index: usize,
    pub vaddr: u64,
    pub paddr: u64,
    pub memsz: u64,
    pub filesz: u64,
    pub read: bool,
    pub write: bool,
    pub exec: bool,
}

/// What a symbol is
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum SymKind {
    Func,
    Object,
    Other,
}

/// A symbol of `.symtab`
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Symbol {
    /// As in the ELF (`_ZN4core3fmt5write17h..E`)
    pub raw: String,
    /// Demangled without the hash (`core::fmt::write`)
    pub name: String,
    /// The address; bit 0 (Thumb) cleared for functions
    pub addr: u64,
    pub size: u64,
    pub kind: SymKind,
    /// The output section it is in
    pub section: Option<String>,
}

/// A function's code as [`thumb`] reads it
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct FnCode {
    /// Index into [`Elf::symbols`]
    pub symbol: usize,
    /// Bytes the prologue takes from the stack, `None` when it has none that was recognized
    pub frame: Option<u32>,
    /// Indices into [`Elf::symbols`] of the functions it calls (`bl`, `b.w` out of the function)
    pub calls: Vec<usize>,
    /// Calls through a pointer
    pub indirect: u32,
}

/// The ELF as the checks see it
#[derive(Clone, Debug, Serialize)]
pub struct Elf {
    /// As given, or relative to the crate directory when it is inside
    pub rel: String,
    #[serde(skip)]
    pub path: PathBuf,
    pub machine: Machine,
    pub sections: Vec<Section>,
    pub segments: Vec<Segment>,
    #[serde(skip)]
    pub symbols: Vec<Symbol>,
    /// Arm Thumb only: one per function symbol with a size, by address
    #[serde(skip)]
    pub code: Vec<FnCode>,
}

impl Elf {
    /// Reads the ELF at `path`; `dir` is the crate directory, for [`Elf::rel`]
    pub fn load(path: &Path, dir: &Path) -> Result<Elf, String> {
        let data = std::fs::read(path).map_err(|e| format!("can't read: {e}"))?;
        let rel = path
            .strip_prefix(dir)
            .map_or_else(|_| path.display().to_string(), |p| p.display().to_string());
        Elf::parse(&data, rel, path.to_path_buf())
    }

    pub fn parse(data: &[u8], rel: String, path: PathBuf) -> Result<Elf, String> {
        let header = elf::FileHeader32::<object::Endianness>::parse(data)
            .map_err(|e| format!("not a 32-bit ELF: {e}"))?;
        let endian = header.endian().map_err(|e| e.to_string())?;
        let machine = match header.e_machine(endian) {
            elf::EM_ARM => Machine::Arm,
            elf::EM_RISCV => Machine::RiscV,
            elf::EM_XTENSA => Machine::Xtensa,
            m => Machine::Other(m),
        };

        let mut segments = Vec::new();
        for (index, ph) in header
            .program_headers(endian, data)
            .map_err(|e| e.to_string())?
            .iter()
            .enumerate()
        {
            if ph.p_type(endian) != elf::PT_LOAD {
                continue;
            }
            let f = ph.p_flags(endian);
            segments.push(Segment {
                index,
                vaddr: ph.p_vaddr(endian).into(),
                paddr: ph.p_paddr(endian).into(),
                memsz: ph.p_memsz(endian).into(),
                filesz: ph.p_filesz(endian).into(),
                read: f & elf::PF_R != 0,
                write: f & elf::PF_W != 0,
                exec: f & elf::PF_X != 0,
            });
        }

        let table = header.sections(endian, data).map_err(|e| e.to_string())?;
        let mut sections = Vec::new();
        let mut by_index: BTreeMap<usize, usize> = BTreeMap::new();
        for (i, sh) in table.iter().enumerate() {
            let flags = sh.sh_flags(endian);
            let name =
                String::from_utf8_lossy(table.section_name(endian, sh).unwrap_or(b"")).into_owned();
            if name.is_empty() {
                continue;
            }
            let addr: u64 = sh.sh_addr(endian).into();
            let size: u64 = sh.sh_size(endian).into();
            let alloc = flags & elf::SHF_ALLOC != 0;
            let load = segments
                .iter()
                .find(|s| {
                    alloc && s.filesz > 0 && addr >= s.vaddr && addr < s.vaddr + s.memsz.max(1)
                })
                .map_or(addr, |s| s.paddr + (addr - s.vaddr));
            by_index.insert(i, sections.len());
            sections.push(Section {
                name,
                addr,
                size,
                load,
                alloc,
                write: flags & elf::SHF_WRITE != 0,
                exec: flags & elf::SHF_EXECINSTR != 0,
                nobits: sh.sh_type(endian) == elf::SHT_NOBITS,
            });
        }

        let mut symbols = Vec::new();
        if let Ok(st) = table.symbols(endian, data, elf::SHT_SYMTAB) {
            for sym in st.iter() {
                let raw = String::from_utf8_lossy(sym.name(endian, st.strings()).unwrap_or(b""))
                    .into_owned();
                if raw.is_empty() || raw.starts_with('$') {
                    continue;
                }
                let kind = match sym.st_type() {
                    elf::STT_FUNC => SymKind::Func,
                    elf::STT_OBJECT => SymKind::Object,
                    elf::STT_SECTION | elf::STT_FILE => continue,
                    _ => SymKind::Other,
                };
                let mut addr: u64 = sym.st_value(endian).into();
                if kind == SymKind::Func && machine == Machine::Arm {
                    addr &= !1;
                }
                let shndx = sym.st_shndx(endian) as usize;
                let section = by_index.get(&shndx).map(|&i| sections[i].name.clone());
                symbols.push(Symbol {
                    name: demangle(&raw),
                    raw,
                    addr,
                    size: sym.st_size(endian).into(),
                    kind,
                    section,
                });
            }
        }
        symbols.sort_by(|a, b| (a.addr, &a.raw).cmp(&(b.addr, &b.raw)));
        symbols.dedup_by(|a, b| a.addr == b.addr && a.raw == b.raw);

        let mut out = Elf {
            rel,
            path,
            machine,
            sections,
            segments,
            symbols,
            code: Vec::new(),
        };
        if machine == Machine::Arm {
            out.code = out.read_code(data, &table, endian);
        }
        Ok(out)
    }

    fn read_code(
        &self,
        data: &[u8],
        table: &object::read::elf::SectionTable<'_, elf::FileHeader32<object::Endianness>>,
        endian: object::Endianness,
    ) -> Vec<FnCode> {
        // the bytes of each executable section, by its run address
        let mut images: Vec<(u64, &[u8])> = Vec::new();
        for sh in table.iter() {
            let flags = sh.sh_flags(endian);
            if flags & elf::SHF_ALLOC == 0 || sh.sh_type(endian) == elf::SHT_NOBITS {
                continue;
            }
            if let Ok(bytes) = sh.data(endian, data) {
                images.push((sh.sh_addr(endian).into(), bytes));
            }
        }
        let bytes_at = |addr: u64, len: u64| -> Option<&[u8]> {
            images.iter().find_map(|(a, b)| {
                let off = addr.checked_sub(*a)? as usize;
                b.get(off..off + len as usize)
            })
        };
        let literal =
            |addr: u64| bytes_at(addr, 4).map(|w| u32::from_le_bytes([w[0], w[1], w[2], w[3]]));
        let mut by_addr: BTreeMap<u64, usize> = BTreeMap::new();
        for (i, s) in self.symbols.iter().enumerate() {
            if s.kind == SymKind::Func {
                by_addr.entry(s.addr).or_insert(i);
            }
        }
        let mut out = Vec::new();
        for (&addr, &i) in &by_addr {
            let s = &self.symbols[i];
            let exec = s
                .section
                .as_ref()
                .and_then(|n| self.section(n))
                .is_some_and(|x| x.exec);
            if s.size == 0 || !exec {
                continue;
            }
            let Some(code) = bytes_at(addr, s.size) else {
                continue;
            };
            let c = thumb::read(addr, code, literal);
            out.push(FnCode {
                symbol: i,
                frame: c.frame,
                calls: c
                    .calls
                    .iter()
                    .filter_map(|t| by_addr.get(t).copied())
                    .collect(),
                indirect: c.indirect,
            });
        }
        out
    }

    pub fn section(&self, name: &str) -> Option<&Section> {
        self.sections.iter().find(|s| s.name == name)
    }

    /// The first symbol with this raw or demangled name
    pub fn symbol(&self, name: &str) -> Option<&Symbol> {
        self.symbols
            .iter()
            .find(|s| s.raw == name || s.name == name)
    }

    /// The code of the function symbol `i`
    pub fn code_of(&self, symbol: usize) -> Option<&FnCode> {
        self.code.iter().find(|c| c.symbol == symbol)
    }

    /// Functions that call symbol `i`, as symbol indices
    pub fn callers(&self, symbol: usize) -> Vec<usize> {
        self.code
            .iter()
            .filter(|c| c.calls.contains(&symbol))
            .map(|c| c.symbol)
            .collect()
    }

    /// Sections that take memory at run time and have a size
    pub fn alloc_sections(&self) -> impl Iterator<Item = &Section> {
        self.sections.iter().filter(|s| s.alloc && s.size > 0)
    }
}

/// The deepest stack from each function down its direct calls (Thumb code), with the path. A call back into a
/// function on the path (recursion) ends the path there and is recorded; calls through pointers are not followed.
pub struct CallDepths<'e> {
    elf: &'e Elf,
    /// symbol index → index into `elf.code`
    code: HashMap<usize, usize>,
    memo: HashMap<usize, (u64, Option<usize>)>,
    on_path: Vec<usize>,
    recursive: Vec<usize>,
}

impl<'e> CallDepths<'e> {
    pub fn new(elf: &'e Elf) -> CallDepths<'e> {
        CallDepths {
            elf,
            code: elf
                .code
                .iter()
                .enumerate()
                .map(|(i, c)| (c.symbol, i))
                .collect(),
            memo: HashMap::new(),
            on_path: Vec::new(),
            recursive: Vec::new(),
        }
    }

    /// The deepest stack from function symbol `sym` down its calls: its frame plus the deepest callee's
    pub fn depth(&mut self, sym: usize) -> u64 {
        if let Some(&(d, _)) = self.memo.get(&sym) {
            return d;
        }
        let Some(&ci) = self.code.get(&sym) else {
            return 0;
        };
        if self.on_path.contains(&sym) {
            self.recursive.push(sym);
            return 0;
        }
        self.on_path.push(sym);
        let c = &self.elf.code[ci];
        let mut best = (0, None);
        for &callee in &c.calls {
            let d = self.depth(callee);
            if d > best.0 || best.1.is_none() {
                best = (d.max(best.0), Some(callee));
            }
        }
        self.on_path.pop();
        let d = c.frame.unwrap_or(0) as u64 + best.0;
        self.memo.insert(sym, (d, best.1));
        d
    }

    /// The deepest path from `sym` (after [`CallDepths::depth`] of it), as symbol indices
    pub fn path(&self, mut sym: usize) -> Vec<usize> {
        let mut out = vec![sym];
        while let Some(&(_, Some(next))) = self.memo.get(&sym) {
            if out.contains(&next) {
                break;
            }
            out.push(next);
            sym = next;
        }
        out
    }

    /// [`CallDepths::path`] as names, the middle left out past 12 steps
    pub fn path_names(&self, sym: usize) -> Vec<&'e str> {
        let p = self.path(sym);
        let mut out: Vec<&'e str> = p
            .iter()
            .map(|&i| self.elf.symbols[i].name.as_str())
            .collect();
        if out.len() > 12 {
            out.splice(6..out.len() - 5, ["…"]);
        }
        out
    }

    /// Functions found calling back into their own path (recursion), as symbol indices
    pub fn recursive(&self) -> &[usize] {
        &self.recursive
    }
}

/// Places of the ELF's facts for findings: the memory.x region, the source of a static or function, the ELF itself
impl super::Firmware {
    /// The memory.x region holding `addr`
    pub fn region_at(&self, addr: u64) -> Option<&Region> {
        self.memory.iter().find(|r| r.contains(addr))
    }

    /// The line of a region in memory.x
    pub fn region_span(&self, r: &Region) -> Span {
        let file = self
            .memory_x
            .as_ref()
            .map_or("memory.x", |m| m.rel.as_str());
        Span::new(file, r.line).label(format!("{} region", r.name))
    }

    /// The ELF, with what the finding is about as the label
    pub fn elf_span(&self, label: impl Into<String>) -> Span {
        let file = self.elf.as_ref().map_or("<elf>", |e| e.rel.as_str());
        Span::new(file, 0).label(label)
    }

    /// Where the symbol is defined in the source (a static or a function of the same path), else the ELF
    pub fn symbol_span(&self, sym: &Symbol) -> Span {
        let loc = self
            .facts
            .statics
            .iter()
            .find(|s| s.path == sym.name)
            .map(|s| &s.loc)
            .or_else(|| {
                self.facts
                    .functions
                    .iter()
                    .find(|f| f.path == sym.name)
                    .map(|f| &f.loc)
            });
        match loc {
            Some(l) => Span::at(&l.file, l.line, l.column).label(sym.name.clone()),
            None => self.elf_span(sym.name.clone()),
        }
    }
}

/// `n` bytes for a message: `812 B`, `4.5 KiB`
pub fn bytes(n: u64) -> String {
    if n < 1024 {
        format!("{n} B")
    } else if n.is_multiple_of(1024) {
        format!("{} KiB", n / 1024)
    } else {
        format!("{:.1} KiB", n as f64 / 1024.0)
    }
}

/// `_ZN4core3fmt5write17h0123456789abcdefE` → `core::fmt::write`; other names as they are
pub fn demangle(raw: &str) -> String {
    format!("{:#}", rustc_demangle::demangle(raw))
}

#[cfg(test)]
mod tests {
    #[test]
    fn demangles_without_the_hash() {
        assert_eq!(
            super::demangle("_ZN4core3fmt5write17h0123456789abcdefE"),
            "core::fmt::write"
        );
        assert_eq!(super::demangle("_stack_start"), "_stack_start");
    }
}
