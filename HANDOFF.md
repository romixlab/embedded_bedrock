# Handoff: sync-converge (21 Oct 2026)

## Done (committed on s/21oct2026-sync-converge in all five repos)

- embedded_bedrock a8ae73d: bedrock_sync `State::Acquiring` until slew left + source error <= `Config::lock_within_us` (2 ms), `Locked` within, back beyond 2x or on a step; slew 5 % while acquiring, 1 % locked; `slew_left_at`; `ClockServer::sync_status` fills `slew_us`; closed-loop test `bedrock_sync/tests/closed_loop.rs` (dev-dep ww_clock_sync): 250 ms off within 2 ms in 5.0 s. `bedrock_led::look` (DEV-15): OK / BUSY / WARN / FAULT / ALERT / HEARTBEAT / IDENTIFY, palette, `Look::under`.
- wire_weaver d19c1f54: `SyncStatus::slew_us`, states documented (trait + docs/std_library/ww_timebase.md#states), `ww_clock_sync::Options::acquire` (`Pace`, 10 Hz until Locked), `ww sync` convergence view (`--within-us`, events, LED phase), snapshot saved.
- usb_io_fw 3d1e0cf (rePort 4 s -> 3 s breath, `look::OK`), typec_dongle 8236a19 (`look::OK` / `HEARTBEAT`, Low peak 48 was 40), vb135 e1748be (looks from `look`, boot codes stay with a reason).
- (d) B135 `run_with` was already on main (a138f41) and on the board; nothing to change.
- Skill tree: no ww signals exist; filed P2618#21oct-2045.

## Hardware so far (omarchy-m1)

- rePort and B135B flashed from this branch. `ww sync`: rePort converged 1.6-1.8 s, B135 1.6 s; LED phase 18-147 µs apart (±1.6 ms, rePort's ~2 ms RTT).
- Donguru (B153A): held by session donguru-hub-keep for ~2 h from 21:00; it says when free. Then: `just update SERIAL` from typec_dongle (with this ww on PATH: `cargo install --path wire_weaver_cli --root $CLAUDE_JOB_DIR/tmp/ww`), `ww sync` with all three, note per-board convergence and LED phase; hand it back.

## Next

- Donguru hardware run, numbers into FEATURES (DEV-13/15, TOOL-16, FW-44) and the report; `git rm HANDOFF.md`; `tpm work ready`; `tpm work say parent` with the visible-change list.
- No auto-sync child (parent: folded into ww-hub planning).
