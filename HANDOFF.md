# Handoff: gen-host-crates (P3, P2528#24oct-1609c, TPL-37 crates)

Plan: ~/git/tpm/maintain/P2528-embedded-bedrock/design/g0-gen-suite.md (piece P3). Reference: nucleo_g0b1re_template cc1c30b at /tmp/g0ref-gen (re-create: `git -C ~/git/nucleo_g0b1re_template worktree add --detach /tmp/g0ref-gen cc1c30b`). Shell PATH lacks cargo: `export PATH=~/.cargo/bin:~/.local/bin:$PATH`. Compare: `uv run firmware_template_skill/scripts/bedrock_gen.py new --answers /tmp/g0ref-gen/bedrock_fw.json --board nucleo_g0b1re --out /tmp/ghc/g0`, then `diff -r /tmp/g0ref-gen/nucleo_g0b1re_<client|cli|py> /tmp/ghc/g0/nucleo_g0b1re_<...>`.

## Branch state

- Rebased onto gen-device-suite's branch (P4, s/24oct2026-gen-device-suite: 4bce811 context, 47ce06f API device contract). HANDOFF.md on this branch was P4's; this file replaces it. Rebase again onto P4's newest before `tpm work ready` (expect a HANDOFF.md conflict: take ours, it is `git rm`ed at the end anyway).
- d83faf6: G0's client/cli/py files copied verbatim as `templates/suite/{client,cli,py}/**.j2` (so the parameterisation is a reviewable diff).
- 883db23 + the step commit after it: templates parameterised and wired.

## Done

- Agreed context with P4 (via tpm work say): `s.dev` dict of Device item -> bool (firmware, last_reset, board, uid, label, counters, log, variant, settings, boot, dfu), `s.has_device`, `s.led_at_boot`, `s.variant` {dev_id, device, revisions [(name, rev_id, marking)], flash_kib, part, doc} or None, `s.uid` {doc, ...}, `s.product`, `s.revision`. P4 also has a `doc` filter (`///` lines, width 116); mine is `rewrap(prefix)` as a `{% filter %}` block (any prefix, also `//!`), same width 116.
- bedrock_gen.py: filters `rewrap`, `and_join`, `rust_use` (use list formatted as rustfmt does, 100 cols); `host_names()` adds client_struct, probe_const, probe_kind(_a) (preset [board] probe_kind / probe_kind_a, else PROBE_KINDS per family), py_board, board_short, board_revision, led_name (preset [readme] led), uid_desc, version, variant_example; has_client/has_cli/has_py = o.host; suite_files renders the 12 crate files; post_step `uv lock` in <name>_py.
- Root Cargo.toml.j2: [workspace.dependencies] per s.dev (ww_dep macro, so --ww-src paths work), version from s.version. versions.toml: host crate versions as G0 (cnt_core, defmt-decoder, anstream, anstyle, anyhow, clap, clap_complete, indicatif, pyo3, thiserror, tokio, probe-rs, nusb, maturin).
- Transport: RTT -> `Probe` + `PROBE_RS_CHIP` + `<CLIENT>_PROBE` const, `connect(&Probe)` (G0 verbatim); USB -> `Usb { vid_pid, serial }` + `USB_ID` (c0de:cafe, the firmware's default), `connect(&Usb)`, CLI `--device` (env `<ENV>_DEVICE`, completion via nusb `list_devices().wait()`), `devices` subcommand instead of `probes`, py `Device(serial=None, elf=None)`. Decision: one struct per transport (a firmware has exactly one WW transport), not an enum with a variant that always fails.
- Items left out by the answers leave their methods, subcommands, imports, deps and doc words out (label, last_reset, counters + BKP buffer only with o.bkp, log, variant, settings / led_at_boot, boot, dfu; `--elf` with USB only when counters or log).
- G0 regenerated: renders; host crates differ from G0 only in wording / reflow (53 diff lines): title "Nucleo-G0B1RE" vs G0's "Nucleo G0B1RE" (accepted in P2), probe_name "own" vs "on-board", greedy 116 wrap vs G0's hand wraps, version 0.1.0 vs 0.1.5, P4's stm32.toml variant data ("STM32G0B1 / G0C1", revisions Z and Y too), and 3 extra blank lines in device.rs (bug, below).

## Next

1. Fix the 3 extra blank lines at the end of client/src/device.rs.j2 (InOrder / resolve_elf / decode / tests: an `{% endif %}{% if %}` pair plus the original blank line). Check device.py docstring (needs `rewrap("")`-like wrap or a fixed form; G0 has 4 lines).
2. Build: `cd /tmp/ghc/g0 && cargo test --workspace --locked && cargo clippy --workspace --all-targets --locked -- -D warnings && cargo fmt --all --check && just test-py`. Nothing compiled yet: expect fixes (rustfmt of conditional bits, unused imports when items are off).
3. Matrix --host entries: G0 rtt preset (above), G0 usb (`new g0usb --chip STM32G0B1RE --wire-weaver --ww-transport usb --host --settings --counters --led PA5`), one RP usb (`--chip RP2040 ... --ww-transport usb --host`); each: cargo test --workspace, clippy -D warnings, just test-py, rustfmt. Also a no-settings / no-counters variant to check left-out items compile. Add entries to references/frameworks.md tested matrix.
4. README/AGENTS/justfile/ci text that P2 put behind has_client/has_py now renders: diff with G0 (ci python job, .gitignore python lines).
5. FEATURES.md TPL-37 ✅ (crates part) with pointers; template CHANGELOG [Unreleased] entry + upgrade notes (new crates appear with --host; SKILL.md options/layout); root CHANGELOG not needed (template only).
6. Rebase onto P4's branch, `just test --changed`, `just lint`, `git rm HANDOFF.md`, commit (squash wip commits into one feat(template) commit), `tpm work ready`.

## Gotchas

- Never touch the J-Link 1366:0105 (it only appears as a string in the probe_select test, as in G0). No hardware runs needed for P3's done criteria.
- Bash tool cwd jumps around: use absolute paths.
- Jinja: trim_blocks/lstrip_blocks; `{% filter rewrap("    /// ") %}` around a paragraph written on one line. `sort(case_sensitive=true)` for Rust use lists (or the `rust_use` filter).
