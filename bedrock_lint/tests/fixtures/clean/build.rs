fn main() {
    println!("cargo:rustc-link-arg=-Tlink.x");
    println!("cargo::rerun-if-changed=memory.x");
}
