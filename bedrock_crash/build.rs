fn main() {
    // ARMv6-M and ARMv8-M Baseline have no CFSR / HFSR / MMFAR / BFAR (reading them is not defined)
    println!("cargo::rustc-check-cfg=cfg(bedrock_baseline)");
    let target = std::env::var("TARGET").unwrap_or_default();
    if target.starts_with("thumbv6m") || target.starts_with("thumbv8m.base") {
        println!("cargo:rustc-cfg=bedrock_baseline");
    }
}
