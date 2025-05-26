use std::{env, path::PathBuf};

fn main() {
    println!("cargo:rerun-if-env-changed=CNT_BUFFER_SIZE_WORDS");

    let size = env::var("CNT_BUFFER_SIZE_WORDS")
        .map(|s| {
            s.parse()
                .expect("could not parse CNT_BUFFER_SIZE_WORDS as usize")
        })
        .unwrap_or(64_usize);

    let out_dir_path = PathBuf::from(env::var_os("OUT_DIR").unwrap());
    let out_file_path = out_dir_path.join("consts.rs");

    std::fs::write(
        out_file_path,
        format!(
            "/// Counters buffer size (default: 64 words = 256 bytes).
            ///
            /// Can be customized by setting the `CNT_BUFFER_SIZE_WORDS` environment variable.
            /// Use a power of 2 for best performance.
            pub(crate) const BUF_SIZE: usize = {};",
            size
        ),
    )
    .unwrap();
}
