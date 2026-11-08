//! Code bytes per source file, from the DWARF line table of a linked ELF.
//!
//! Each row of the line table gives the source location of the instructions from its address to the next row's;
//! with inlining that is the innermost inlined function's file, so a driver inlined into a task counts as the
//! driver's. LTO merges crates into few functions, which a symbol list can't split; the line table can. Only
//! addresses inside executable sections count (dead code that the linker dropped keeps line rows at address 0).
//! Literal pools in Thumb code fall under the row before them, the function they belong to.
//!
//! The ELF needs debug info (`debug = 2` or `line-tables-only`); a stripped one has no line table.

use object::read::elf::{ElfFile32, ElfFile64};
use object::{Endianness, Object, ObjectSection, SectionFlags, elf};
use std::collections::BTreeMap;

/// Code bytes by source file.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct CodeSize {
    /// Bytes of the executable sections.
    pub exec: u64,
    /// Bytes the line table attributes to a file, by file path as the compiler wrote it.
    pub by_file: BTreeMap<String, u64>,
}

impl CodeSize {
    pub fn attributed(&self) -> u64 {
        self.by_file.values().sum()
    }

    /// Bytes by [crate_of] the file.
    pub fn by_crate(&self) -> BTreeMap<String, u64> {
        let mut m = BTreeMap::new();
        for (f, b) in &self.by_file {
            *m.entry(crate_of(f)).or_default() += b;
        }
        m
    }
}

/// Reads the line table of an ELF.
pub fn code_size(data: &[u8]) -> Result<CodeSize, String> {
    match object::FileKind::parse(data).map_err(|e| e.to_string())? {
        object::FileKind::Elf32 => {
            let f = ElfFile32::<Endianness>::parse(data).map_err(|e| e.to_string())?;
            from_object(&f)
        }
        object::FileKind::Elf64 => {
            let f = ElfFile64::<Endianness>::parse(data).map_err(|e| e.to_string())?;
            from_object(&f)
        }
        k => Err(format!("not an ELF: {k:?}")),
    }
}

fn from_object<'d>(file: &impl Object<'d>) -> Result<CodeSize, String> {
    let mut exec: Vec<(u64, u64)> = Vec::new();
    for s in file.sections() {
        if let SectionFlags::Elf { sh_flags } = s.flags()
            && sh_flags & u64::from(elf::SHF_EXECINSTR) != 0
            && sh_flags & u64::from(elf::SHF_ALLOC) != 0
            && s.size() > 0
        {
            exec.push((s.address(), s.address() + s.size()));
        }
    }
    let endian = if file.is_little_endian() {
        gimli::RunTimeEndian::Little
    } else {
        gimli::RunTimeEndian::Big
    };
    let load =
        |id: gimli::SectionId| -> Result<gimli::EndianSlice<'d, gimli::RunTimeEndian>, String> {
            let data = match file.section_by_name(id.name()) {
                Some(s) => s.data().map_err(|e| e.to_string())?,
                None => &[],
            };
            Ok(gimli::EndianSlice::new(data, endian))
        };
    let dwarf = gimli::Dwarf::load(load)?;
    if gimli::Section::reader(&dwarf.debug_line).is_empty() {
        return Err("no .debug_line: build with debug info (debug = 2) and don't strip".into());
    }
    let mut out = CodeSize {
        exec: exec.iter().map(|(a, b)| b - a).sum(),
        ..CodeSize::default()
    };
    let inside = |a: u64| exec.iter().any(|&(s, e)| a >= s && a < e);
    let mut units = dwarf.units();
    while let Some(header) = units.next().map_err(|e| e.to_string())? {
        let unit = dwarf.unit(header).map_err(|e| e.to_string())?;
        let Some(program) = unit.line_program.clone() else {
            continue;
        };
        let mut rows = program.rows();
        // the row before, open until the next row (or the end of its sequence) gives its length
        let mut open: Option<(u64, u64)> = None; // (address, file index)
        let mut names: BTreeMap<u64, String> = BTreeMap::new();
        while let Some((header, row)) = rows.next_row().map_err(|e| e.to_string())? {
            if let Some((start, file)) = open.take()
                && row.address() > start
                && inside(start)
            {
                let name = match names.get(&file) {
                    Some(n) => n.clone(),
                    None => {
                        let n = file_name(&dwarf, &unit, header, file);
                        names.insert(file, n.clone());
                        n
                    }
                };
                *out.by_file.entry(name).or_default() += row.address() - start;
            }
            if !row.end_sequence() {
                open = Some((row.address(), row.file_index()));
            }
        }
    }
    Ok(out)
}

fn file_name<R: gimli::Reader>(
    dwarf: &gimli::Dwarf<R>,
    unit: &gimli::Unit<R>,
    header: &gimli::LineProgramHeader<R>,
    index: u64,
) -> String {
    let Some(f) = header.file(index) else {
        return "?".into();
    };
    let s = |a: gimli::AttributeValue<R>| -> String {
        dwarf
            .attr_string(unit, a)
            .ok()
            .and_then(|r| r.to_string_lossy().ok().map(|c| c.into_owned()))
            .unwrap_or_default()
    };
    let name = s(f.path_name());
    if name.starts_with('/') {
        return name;
    }
    let mut dir = f.directory(header).map(s).unwrap_or_default();
    if !dir.starts_with('/') {
        // relative to the compilation directory
        let comp = unit
            .comp_dir
            .as_ref()
            .and_then(|c| c.to_string_lossy().ok().map(|c| c.into_owned()))
            .unwrap_or_default();
        dir = match (comp.is_empty(), dir.is_empty()) {
            (true, _) => dir,
            (false, true) => comp,
            (false, false) => format!("{}/{dir}", comp.trim_end_matches('/')),
        };
    }
    if dir.is_empty() {
        name
    } else {
        format!("{}/{name}", dir.trim_end_matches('/'))
    }
}

/// The crate a source path belongs to: `name-x.y.z` from a registry, the checkout's directory from a git
/// dependency, `core` / `alloc` / `std` from the toolchain, otherwise the directory before `src/` (a path
/// dependency or the firmware itself).
pub fn crate_of(path: &str) -> String {
    let parts: Vec<&str> = path.split('/').collect();
    if let Some(i) = parts.iter().position(|p| *p == "registry") {
        // .../registry/src/<index>/<crate-x.y.z>/...
        if let Some(c) = parts.get(i + 3) {
            return (*c).to_string();
        }
    }
    if let Some(i) = parts.iter().position(|p| *p == "checkouts") {
        // .../git/checkouts/<repo-hash>/<rev>/<crate dir>/src/...
        if let Some(src) = parts.iter().skip(i).position(|p| *p == "src") {
            return parts[i + src - 1].to_string();
        }
    }
    if let Some(i) = parts.iter().position(|p| *p == "library")
        && path.contains("/rustc/")
        && let Some(c) = parts.get(i + 1)
    {
        return (*c).to_string();
    }
    match parts.iter().rposition(|p| *p == "src") {
        Some(i) if i > 0 => parts[i - 1].to_string(),
        _ => path.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::crate_of;

    #[test]
    fn crates_from_paths() {
        assert_eq!(
            crate_of(
                "/home/u/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/embassy-stm32-0.6.0/src/usb/usb.rs"
            ),
            "embassy-stm32-0.6.0"
        );
        assert_eq!(
            crate_of("/rustc/17067e9ac6d7e98f18d4ff3e3b6e0ac2d2e7e5c4/library/core/src/ptr/mod.rs"),
            "core"
        );
        assert_eq!(
            crate_of("/home/u/.cargo/git/checkouts/wire_weaver-0f3e/1a2b3c4/ww_device/src/lib.rs"),
            "ww_device"
        );
        assert_eq!(
            crate_of("/home/u/git/embedded_bedrock/bedrock_usb/src/v4/driver.rs"),
            "bedrock_usb"
        );
        assert_eq!(crate_of("src/main.rs"), "src/main.rs");
    }
}
