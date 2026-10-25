//! The API compared with its snapshots in `api_snapshots/` (`just api-check`): fails when it changed without the
//! version bump the change needs, or when there is no snapshot to compare with. WireWeaver's
//! docs/evolution/checker_tool.md explains the check, docs/evolution/rules.md the rules.

#[test]
fn api_evolution() {
    wire_weaver_evolution::Check::new(env!("CARGO_MANIFEST_DIR"))
        .strict(true)
        .assert();
}
