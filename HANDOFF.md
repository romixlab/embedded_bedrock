# Handoff: gen-device-suite (P4, P2528#24oct-1609b, TPL-41)

Task: --wire-weaver firmware serves the device contract as nucleo_g0b1re_template cc1c30b does. Plan: ~/git/tpm/maintain/P2528-embedded-bedrock/design/g0-gen-suite.md (P4 row). Reference checkout: /tmp/g0ref-gen (cc1c30b; re-create with `git -C ~/git/nucleo_g0b1re_template worktree add --detach /tmp/g0ref-gen cc1c30b`). Shell PATH: `export PATH=~/.cargo/bin:~/.local/bin:$PATH`. Compare: `uv run firmware_template_skill/scripts/bedrock_gen.py new --answers /tmp/g0ref-gen/bedrock_fw.json --board nucleo_g0b1re --out $CLAUDE_JOB_DIR/tmp/g0` then `diff -r` against /tmp/g0ref-gen.

## Done

- Step 1 (commit "wip: device suite context"): rom_boot family, boot word out of BKPSRAM / CNT_BKP_BUFFER_SIZE_WORDS, stm32.toml `[variant.<dev_id>]` tables (467, 483, 450), suite_context `s.dev` etc.
- Step 2 (commit "wip: API crate serves the device contract"): `templates/ww/api/src/lib.rs.j2` and `Cargo.toml.j2` with `device: Device`, Settings, Dfu, Variant, SiliconRevision per `s.dev`. Jinja filter `doc` (bedrock_gen.py `doc_comment`: wraps at 116, `marker="//!"`/`"//"`, `hang` for list items, keeps a preset's own line breaks). Preset `[variant] doc`. Explained diffs vs G0: version 0.1.0 and no `#[since]`/0.2 paragraph (new projects start flat-free), Z and Y revisions (ES0548 Rev 4 lists them, G0 only A).
- Step 3 (commit "wip: firmware serves the device contract"): `app/src/ww.rs.j2` rewritten (G0's server: select/select3 per log+settings, streams, LinkEvent, uid per family, `ServerState::new`), new `app/src/ww/device.rs.j2`, `app/src/log.rs.j2` (RTT: returns ww_up/ww_down; USB: only the defmt channel), `app/src/boot.rs.j2` (`s.boot`: word, usb, healthy, rescue from preset `[rom_boot]`), main.rs / _rtt_init / _embassy_loop / embassy-stm32 / embassy-rp (RP2040 flash uid read in main, passed as `uid`), settings.rs (led_at_boot slot 1, reset_to_defaults), firmware Cargo.toml deps. `sort_uses` now puts self/super/crate first (rustfmt). **The G0 answers regenerate G0's firmware/src byte for byte; dev and release build.**
- Decision: `device.boot`/`dfu` needs `--log defmt` (rom_boot_family returns "" otherwise; boot.rs logs defmt values). File a FEATURES follow-up for other logs.
- suite_context additions told to gen-host-crates: `s.variant.{flash_kib, part, doc}`. Not yet told: `s.boot` (word, usb, healthy, rescue), `s.uid.{expr, serial, arg}` — tell them.

## Next

1. `tpm work say gen-host-crates` about `s.boot` and `s.uid.expr/serial/arg`.
2. Matrix (task 5): g0rtt, g0usb, g0set, h725 (+ h725 without --bootloader: rom_boot stm32h7), rp2040 (ww), nrf840, G0 --board --host; dev + release + `rustfmt --edition 2024 --check` on every .rs. Untested so far: everything but the G0 answers. Expect fixes in ww.rs.j2 for USB (LinkEvent import from `wire_weaver_usb_embassy::ww_device`), no-settings / no-log / log rtt paths, nRF `device_id()` (FICR reg via `s.uid.reg`), RP2350 `chip_id()`, RP2040 `p.FLASH.reborrow()`, H7 variant (`idc`, 12-bit dev_id no mask, no clock line). Pre-existing: USB `start` uses `unwrap!` even without defmt.
3. --ww-src local (task 4): bedrock_rom_boot takes ww_dfu / wire_weaver from git: extend `bedrock_ww_patch` (macros.j2) with those crates when `s.dev.boot`, and emit the patch also without --build-info then. --embassy main: check g0usbm builds (bedrock_rom_boot's embassy-time/sync from crates.io, covered by [patch.crates-io]?).
4. FEATURES TPL-41 (+ follow-up id for boot without defmt via `tpm work claim TPL`), template CHANGELOG [Unreleased] + upgrade notes (API 0.1 -> 0.2 breaking for flat layouts: firmware/label/last_reset move under `device`), SKILL.md, frameworks.md matrix. `git rm HANDOFF.md`, `just test --changed`, `just lint`, `tpm work ready`.

## Gotchas

- Nucleo ST-LINK on omarchy-m1 is shared: ask (`tpm work say parent`) before hardware runs; never touch the J-Link 1366:0105.
- Generate into the job tmp dir, never into a firmware repo. No push.
- The generator sorts `use` runs itself (`sort_uses`); template order of imports doesn't matter within a run.
- firmware/README.md differs from G0 in the setup section (the template's newer `just setup` text) and the BKPSRAM row: not this task's.
