use std::env;

fn main() {
    let out = env::var("OUT_DIR").unwrap();
    println!("cargo:rustc-link-search={out}");
    println!("cargo:rustc-linker=flip-link");
    println!("cargo:rustc-link-arg=-Tlink.x");
    println!("cargo:memory-size=512K");
    println!("cargo::metadata=fine");
}
