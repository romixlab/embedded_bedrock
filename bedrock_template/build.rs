use std::{env, fs};
use std::path::PathBuf;

fn main() {
    // Put `memory.x` in our output directory and ensure it's on the linker search path.
    let out = &PathBuf::from(env::var_os("OUT_DIR").unwrap());
    fs::write(out.join("memory.x"), include_bytes!("memory.x")).unwrap();

    println!("cargo:rustc-link-search={}", out.display());
    println!("cargo:rerun-if-changed=memory.x");

    {% if use_flip_link -%}
    println!("cargo:rustc-linker=flip-link");
    {% endif -%}
    
    {% if use_counters -%}
    println!("cargo:rustc-link-arg=-Tcnt.x");
    {% endif -%}

    bedrock_build::common();

    let info = build_info_build::build_script()
        .collect_dependencies(build_info_build::DependencyDepth::Depth(0))
        .build();
    let info = bedrock_build::serialize_build_info(info);
    let info_file_path = out.join("build_info.rs");
    fs::write(info_file_path, info).unwrap();
}
