//! `boardmap --version`: version, git SHA (`-dirty` with uncommitted changes) and build time.

use std::process::Command;

fn run(cmd: &str, args: &[&str]) -> Option<String> {
    let out = Command::new(cmd).args(args).output().ok()?;
    out.status
        .success()
        .then(|| String::from_utf8_lossy(&out.stdout).trim().to_string())
}

fn main() {
    let sha = match run("git", &["rev-parse", "--short", "HEAD"]) {
        Some(sha) => {
            let dirty = run("git", &["status", "--porcelain"]).is_some_and(|s| !s.is_empty());
            format!("{sha}{}", if dirty { "-dirty" } else { "" })
        }
        None => "unknown".to_string(),
    };
    let built = run("date", &["+%-d %b %Y %H:%M"]).unwrap_or_else(|| "unknown".to_string());
    println!(
        "cargo:rustc-env=BOARDMAP_VERSION={} ({sha}, built {built})",
        env!("CARGO_PKG_VERSION")
    );
    if let Some(git_dir) = run("git", &["rev-parse", "--git-dir"]) {
        println!("cargo:rerun-if-changed={git_dir}/HEAD");
        println!("cargo:rerun-if-changed={git_dir}/index");
    }
}
