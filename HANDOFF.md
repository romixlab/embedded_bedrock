# Handoff: bedrock-steer (device side of SRV-31, one steerer at a time)

## Done

- wire_weaver branch reset onto hub-clock-sync's 03437a19; own commit: `wire_weaver_cli/examples/two_steerers.rs` (bench check: two ClockSyncs with ids 0xA / 0xB on one connection).
- embedded_bedrock `bedrock_sync` (9d0006d): `Steer::steerer`, `Steered::OtherSteerer`, `SharedClock::steerer` / `config`; `ClockServer` keeps followed + noted steerer (24 B names, char-boundary cut), fills `SyncStatus::steerer` / `noted`; `steer` takes `&mut self`. Unit tests in `tests.rs` and `clock.rs`. CHANGELOG + FEATURES DEV-13 updated.
- Boards: handler signatures `SteerPoint<'_>` / `SyncStatus<'_>` in typec_dongle (fw + donguru-core mock), usb_io_fw (fw + client sim), vb135 (fw, b135_dfu test mock, pll.rs test `Steer`). Cargo.lock refreshed.
- wire_weaver fix 91a86a54: `ww_device::link::cut` without a str slice panic path (pulled core's unicode tables, 6.0 KB on B135, enough to overflow B135's `ab` slot); parent took it up. B135 `ab` image now 260,608 of 262,016 B (1,408 B free; master had 6.3 KB free).
- All four repos: `just lint` and `just test` green.
- Numbers: Donguru release (opt-level 3), new wire_weaver with old vs new bedrock_sync: text +1240 B, rodata +304, data +8 (flash +1552 B), bss +144 B. Stack (bedrock_sync for thumbv6m, opt 3, nightly `-Z emit-stack-sizes`): `ClockServer::steer` 88 -> 120 B, `SharedClock::steer_with` 104 -> 112 B, `sync_status` 104 -> 176 B.

## Next

- Hardware: the bench boards dropped off USB at 00:27 on 22 Oct (whole hub disconnected, still gone at 01:35); once back: flash the three, `cargo run -p wire_weaver_cli -- sync`, then `cargo run -p wire_weaver_cli --example two_steerers -- <serial part>` per board; add numbers to FEATURES DEV-13; tell hub-clock-sync when done with the boards.

## Gotchas

- "Before" build trees for the size comparison are detached worktrees under /tmp/bedrock-steer-before (wire_weaver, embedded_bedrock, typec_dongle); remove with `git worktree remove` when done.
