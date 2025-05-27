use std::ffi::OsString;
use std::{env, fs};
use std::path::PathBuf;

pub fn common() {
    println!("cargo:rustc-linker=flip-link");

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