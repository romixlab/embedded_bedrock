use object::{
    Object, ObjectSection, ObjectSymbol, ReadCache, SectionIndex, SectionKind, Symbol, SymbolKind,
    SymbolSection,
};
use std::collections::HashMap;
use std::fs;
use std::path::Path;

pub fn nm_test(file_path: &Path) {
    let file = match fs::File::open(&file_path) {
        Ok(file) => file,
        Err(err) => {
            println!("Failed to open file '{:?}': {}", file_path, err,);
            return;
        }
    };
    let read_cache = ReadCache::new(file);
    // let file = match unsafe { memmap2::Mmap::map(&file) } {
    //     Ok(mmap) => mmap,
    //     Err(err) => {
    //         println!("Failed to map file '{}': {}", file_path, err,);
    //         continue;
    //     }
    // };
    let file = match object::File::parse(&read_cache) {
        Ok(file) => file,
        Err(err) => {
            println!("Failed to parse file '{:?}': {}", file_path, err);
            return;
        }
    };

    let section_kinds = file.sections().map(|s| (s.index(), s.kind())).collect();

    println!(".text = {:08x?}", file.section_by_name(".text"));

    let Some(counters_section) = file.section_by_name(".counters_ram") else {
        return;
    };
    let counters_section_idx = counters_section.index();

    for symbol in file.symbols() {
        if symbol.section_index() != Some(counters_section_idx) {
            continue;
        }
        print_symbol(&symbol, &section_kinds);
    }
}

fn print_symbol<'d>(
    symbol: &Symbol<'d, '_, &'d ReadCache<fs::File>>,
    section_kinds: &HashMap<SectionIndex, SectionKind>,
) {
    if let SymbolKind::Section | SymbolKind::File = symbol.kind() {
        return;
    }

    let mut kind = match symbol.section() {
        SymbolSection::Undefined => 'U',
        SymbolSection::Absolute => 'A',
        SymbolSection::Common => 'C',
        SymbolSection::Section(index) => match section_kinds.get(&index) {
            Some(SectionKind::Text) => 't',
            Some(SectionKind::Data) | Some(SectionKind::Tls) | Some(SectionKind::TlsVariables) => {
                'd'
            }
            Some(SectionKind::ReadOnlyData) | Some(SectionKind::ReadOnlyString) => 'r',
            Some(SectionKind::UninitializedData) | Some(SectionKind::UninitializedTls) => 'b',
            Some(SectionKind::Common) => 'C',
            _ => '?',
        },
        _ => '?',
    };

    if symbol.is_global() {
        kind = kind.to_ascii_uppercase();
    }

    if symbol.is_undefined() {
        print!("{:16} ", "");
    } else {
        print!("{:016x} ", symbol.address());
    }
    println!(
        "{:016x} {} {}",
        symbol.size(),
        kind,
        symbol.name().unwrap_or("<unknown>"),
    );
}
