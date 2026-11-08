//! `bedrock-usb-ch9` driven through its binary. No USB device is attached in CI, so these only cover argument
//! handling and the "not found" path, not an actual check run (that needs real hardware, USBD-7).

use std::process::{Command, Output};

fn bin(args: &[&str]) -> Output {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_bedrock-usb-ch9"));
    for (k, _) in std::env::vars() {
        if k.starts_with("CARGO") || k == "COMPLETE" {
            cmd.env_remove(k);
        }
    }
    cmd.env("NO_COLOR", "1").args(args).output().unwrap()
}

fn stdout(o: &Output) -> String {
    String::from_utf8_lossy(&o.stdout).into_owned()
}

fn stderr(o: &Output) -> String {
    String::from_utf8_lossy(&o.stderr).into_owned()
}

#[test]
fn version_names_the_commit() {
    let o = bin(&["--version"]);
    let s = stdout(&o);
    assert!(s.starts_with("bedrock-usb-ch9 0."), "{s}");
    assert!(s.contains(", commit "), "{s}");
}

#[test]
fn list_with_no_match_says_so_and_exits_clean() {
    // this VID:PID is reserved for Linux's root hubs and vendor-specific testing, never a real peripheral
    let o = bin(&["list", "--vid", "ffff", "--pid", "ffff"]);
    assert_eq!(o.status.code(), Some(0), "{}", stdout(&o));
    assert!(
        stdout(&o).contains("no matching USB device"),
        "{}",
        stdout(&o)
    );
}

#[test]
fn run_without_the_device_exits_2() {
    let o = bin(&["run", "--vid", "ffff", "--pid", "ffff"]);
    assert_eq!(o.status.code(), Some(2), "{}", stdout(&o));
    assert!(
        stderr(&o).contains("no USB device ffff:ffff"),
        "{}",
        stderr(&o)
    );
}

#[test]
fn bad_vid_is_a_usage_error() {
    let o = bin(&["info", "--vid", "zz", "--pid", "0001"]);
    assert_eq!(o.status.code(), Some(2));
}

#[test]
fn only_accepts_the_known_check_names() {
    let o = bin(&[
        "run",
        "--vid",
        "ffff",
        "--pid",
        "ffff",
        "--only",
        "wlength,halt",
    ]);
    // the device isn't found, but the --only value itself must parse before that is checked
    assert_eq!(o.status.code(), Some(2), "{}", stdout(&o));
    assert!(!stderr(&o).contains("invalid value"), "{}", stderr(&o));

    let o = bin(&["run", "--vid", "ffff", "--pid", "ffff", "--only", "nope"]);
    assert_eq!(o.status.code(), Some(2));
    assert!(stderr(&o).contains("invalid value"), "{}", stderr(&o));
}
