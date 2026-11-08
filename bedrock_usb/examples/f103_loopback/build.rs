use std::path::PathBuf;
use std::{env, fs};

fn main() {
    // memory.x on the linker search path (cortex-m-rt's link.x includes it); link arguments here rather than in
    // .cargo/config.toml, so a build from the embedded_bedrock root (`just lint`) links the same way
    let out = PathBuf::from(env::var_os("OUT_DIR").unwrap());
    fs::write(out.join("memory.x"), include_bytes!("memory.x")).unwrap();
    println!("cargo:rustc-link-search={}", out.display());
    println!("cargo:rerun-if-changed=memory.x");
    println!("cargo:rustc-link-arg=--nmagic");
    println!("cargo:rustc-link-arg=-Tlink.x");
    println!("cargo:rustc-link-arg=-Tdefmt.x");
}
