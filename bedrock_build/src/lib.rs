mod build_info;

use base64::Engine;
use base64::prelude::BASE64_STANDARD;
use bedrock_build_info::{COMPACT_INFO_MAGIC, build_info_crc};
use build_info_common::BuildInfo;
use std::ffi::OsString;
use std::path::PathBuf;
use std::{env, fs};

pub fn common() {
    println!("cargo:rerun-if-env-changed=RAM_LINK");

    let out = &PathBuf::from(env::var_os("OUT_DIR").unwrap());
    if env::var_os("RAM_LINK") == Some(OsString::from("1")) {
        fs::write(
            out.join("link_ram.x"),
            include_bytes!("../link_ram_cortex_m.x"),
        )
        .unwrap();
        println!(
            "cargo::warning=⚠️ \x1b[1;33mUsing RAM linking, old code will be run from FLASH on power-cycle"
        );
        println!("cargo:rustc-link-arg=-Tlink_ram.x");
    } else {
        println!("cargo:rustc-link-arg=-Tlink.x"); // provided by cortex-m-rt
    }

    println!("cargo:rustc-link-arg=-Tdefmt.x");

    // This is needed if your flash or ram addresses are not aligned to 0x10000 in memory.x
    // See https://github.com/rust-embedded/cortex-m-quickstart/pull/95
    println!("cargo:rustc-link-arg=--nmagic");

    println!("cargo:rerun-if-changed=../link_ram_cortex_m.x");
}

pub fn serialize_build_info(info: BuildInfo) -> String {
    let (info_full, mut info_pruned) = build_info::shrink_wrap_build_info(info);
    let info_full = BASE64_STANDARD.encode(&info_full);

    let crc = build_info_crc(&info_pruned).to_le_bytes();

    let pruned_size =
        u16::try_from(info_pruned.len()).expect("Compact info should definitely fit into 65K");
    let pruned_size_le = pruned_size.to_le_bytes();
    let magic = COMPACT_INFO_MAGIC.to_be_bytes();
    info_pruned.splice(
        0..0,
        [
            magic[0],
            magic[1],
            magic[2],
            magic[3],
            pruned_size_le[0],
            pruned_size_le[1],
            crc[0],
            crc[1],
            crc[2],
            crc[3],
        ],
    );
    let total_flash_size = info_pruned.len();

    format!(
        "const COMPACT: &'static [u8] = &{info_pruned:?};

/// Build information to be embedded into MCU FLASH, optimized for size by omitting some fields.
/// Size in FLASH with marker, length and CRC is {total_flash_size}B.
/// Ensure to either print it via defmt or use _ = core::hint::black_box(compact()) to ensure it is saved in FLASH.
pub fn compact() -> &'static [u8] {{ core::hint::black_box(&COMPACT[10..]) }}
    
/// Full build information, only saved to the firmware ELF file through defmt string interning.
/// Ensure to either print it via defmt or use _ = core::hint::black_box(full()) to ensure it is saved in ELF.
pub fn full() -> defmt::Str {{ defmt::intern!(\"{info_full}\") }}"
    )
    // let info_full_len = info_full.len();
    // let info_pruned_len = info_pruned.len();
    // let info_full = to_hex_strings(&info_full);
    // let info_pruned = to_hex_strings(&info_pruned);
    //
    // format!(
    //     "/// Build information to be embedded into MCU FLASH, can be optimized for size by omitting some fields
    //     pub const BUILD_INFO_COMPACT: [u8; {info_pruned_len}] = hex!({info_pruned});
    //
    //     /// Full build information, only saved to the firmware ELF file
    //     pub const BUILD_INFO_FULL: [u8; {info_full_len}] = hex!({info_full});
    //     "
    // )
}

// fn to_hex_strings(bytes: &[u8]) -> String {
//     let mut strings = "\n".to_string();
//     for chunk in bytes.chunks(64) {
//         strings += "\"";
//         for byte in chunk {
//             strings += &format!("{:02X}", byte);
//         }
//         strings += "\"\n";
//     }
//     strings
// }
