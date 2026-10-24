# Handoff: g0-gen-suite (B4, P2528#23oct-1259c)

Plan: ~/git/tpm/maintain/P2528-embedded-bedrock/design/g0-gen-suite.md (read it first: architecture, decisions, piece table with task refs). Reference repo: nucleo_g0b1re_template cc1c30b, checked out detached at /tmp/g0ref-gen (re-create with `git -C ~/git/nucleo_g0b1re_template worktree add --detach /tmp/g0ref-gen cc1c30b` if /tmp was cleared). Compare: `uv run firmware_template_skill/scripts/bedrock_gen.py new --answers /tmp/g0ref-gen/bedrock_fw.json --board nucleo_g0b1re --out /tmp/g0gen/b` then `diff -r`. Shell PATH lacks cargo: `export PATH=~/.cargo/bin:~/.local/bin:$PATH` first.

## Done

- Plan, tasks, claims (see plan). This session holds TPL-37, TPL-38, TPL-45 (bind TPL-37 to P2528#24oct-1609c when spawning P3: `tpm work claim TPL-37 --task P2528#24oct-1609c`). TPL-43's claim was held by bedrock-git-deps, already landed: fix it here (FEATURES 🐛 -> ✅, root workspace + lockfile post-step).
- P2 wip commit "suite layout": options `--host` (needs --wire-weaver), `--cli-name`, `--board PRESET`, `--probe VID:PID` (default SETUP_PROBES family VID:PID; rp now 2e8a:000c, udev needles kept separately). `apply_answers` layers parser defaults < preset [answers] < --answers < flags; `orig_defaults` = defaults with the preset, store_true flags the preset sets are left out of `command`.
- Preset `data/boards/nucleo_g0b1re/`: preset.toml ([answers], [board] title/product/revision/url/probe_name/client, [logo], [boardmap], [clock] use+code), readme.md (README board section, verbatim G0), boardmap toml + geometry (moved from bedrock_board/examples/nucleo_g0b1re/, updated to G0's newer copy: fix the pointers in FEATURES.md:346, CHANGELOG.md:56, AGENTS.md:77).
- `suite_context()` (`s`): names, cli, env prefix, probe, probe_name, probe-rs exact chip (`probe_rs_chip`: STM32G0B1RETx), fw_dir, board, logo, has_client/has_cli/has_py False (P3 turns on), members.
- Every --wire-weaver: root `Cargo.toml` (suite/Cargo.toml.j2), API crate without `[workspace]`, dev-dep wire_weaver_evolution, `tests/evolution.rs` (G0's strict form), `.githooks/pre-commit` (exec bit via EXECUTABLE), `scripts/hooks.py`, setup.py git hooks step, app justfile gets probe var + dotenv, hooks/test/api-check/api-save, cnt --probe; `.cargo/config.toml` runner `--probe`; ww/README explains workspace + evolution check. `post_steps()`: runs `cargo generate-lockfile`, prints next lines (api-save, hooks).
- main.rs (embassy-stm32): board [clock] snippet replaces the TODO; `let config` without `mut` when nothing modifies it (was an unused_mut warning, fails `just lint`).
- --host: `suite_files()`: suite justfile (parameterised, install/deploy/py recipes behind has_cli/has_py), docs/logo (logo.typ rendered with s.logo, template_logo/social/pair*/fonts copied, bytes written), docs/board from the preset.
- Checked: G0 regenerated with --board builds (firmware and root), rustfmt clean; dry-run of rp2040 ww, g0 usb, esp32c3, H7 bootloader, nrf --host passes.

## Done in conversation 2

- Parameterised and wired (suite_files): README.md (board prose from the preset: readme-intro.md, readme.md, readme-rom-boot.md with `{cli}`, and [board] template / [readme] plural, variant, led, reset, first_item), LICENSE (s.year, s.holder, LICENSE_HOLDER), scripts/rename.py (OLD_API), .github/workflows/ci.yml + setup.yml (`{% endraw +%}` keeps the newline under trim_blocks), FEATURES.md and CHANGELOG.md skeletons (generated from the options; the project's own afterwards, upgrades never rewrite them), suite/AGENTS.md.j2 prepended to app/AGENTS.md.j2 with --host. app/AGENTS.md.j2 layout now describes the root workspace (TPL-43).
- suite_context(t, o, lay) grew: board_intro/board_readme/board_rom_boot, client_type, settings_size, uid_bits, year, holder, repo, feature_areas, boardmap, and P3/P4 flags has_device, dev_variant, dev_log, rom_boot (all False; P3/P4 turn them on). setup_context gives setup_tools; ww_root_files gets {**ctx, **setup}.
- post_steps: typst logo (G0's three commands; output byte-identical to G0), boardmap render when on PATH, else next lines.
- Checked: G0 regenerated with --board builds (root + firmware); `--host` without a preset renders (G0 usb, rp2040 rtt). With all P3/P4 flags on (job tmp script gen_all_on.py monkeypatches suite_context), README differs from G0 only in one reflowed line and "the Nucleo's" -> probe_name "the Nucleo-64's"; AGENTS only in the TPL-43 layout text; ci/setup.yml identical.

## Done in conversation 3 (P2 finished)

- Matrix checked by hand (gen-matrix P1 not landed): g0usb, g0rtt, g0 (non-ww), g0bi, rp2040, nrf840 (+bootloaders) build, rustfmt clean; --host entries g0board, g0rtth, rp2040h: root `cargo test` and `just lint` pass. Three host entries added to frameworks.md.
- `new` runs `ww api save <api>` when ww is on PATH (the strict evolution test fails otherwise) and with --host `cargo generate-lockfile` in firmware/ (recipes use --locked). gitignore.j2: `.env` at the project root (probe serial), editor dirs with --host, Python ones with has_py. Preset [board] repository -> root Cargo.toml. probe_name default "the ST-LINK" (no doubled id), board plural in comments.
- FEATURES TPL-37 🚧, TPL-38 ✅, TPL-45 ✅, TPL-43 ✅; template CHANGELOG [Unreleased] with upgrade notes; root CHANGELOG (example moved); SKILL.md options and layout. Squashed into one feat(template) commit.
- G0 diff after regeneration (`--answers ... --board nucleo_g0b1re`): what remains is P3 (client/cli/py crates, has_* text in README, justfile, AGENTS, Cargo.toml, ci python job, .gitignore python lines), P4 (boot.rs, log.rs, ww/ device modules, BKP 4 words + boot word, main.rs early/log/led_at_boot, API 0.2), G0's own version 0.1.5, and wording: G0 writes "Nucleo G0B1RE" in a few places, the preset's title is ST's "Nucleo-G0B1RE" (keep; P8 adopts), setup.py docstring reflow and two comments G0 refined by hand.

## Next

1. Spawned on omarchy-m1 from 649f664 (--from-here only works on this PC): gen-device-suite (P4, P2528#24oct-1609b, TPL-41) and gen-host-crates (P3, P2528#24oct-1609c, TPL-37 bound), in parallel against G0's API; P3 rebases onto P4 before ready. Wait for them; P5-P7 after G4/G5 land.
2. P7 (TPL-46, PinDiag) starts from P4's branch once P4 is ready; reference template main 467f889 (v0.1.6, G5 landed, API 0.3.0); ww pins tui in wire_weaver ab32916. P8 reference is the latest template main, not cc1c30b.
3. W4 (evo-current) when it lands: `require_current(true)` in tests/evolution.rs, `api-save` via `WW_API_SAVE=1 cargo test`.
4. P8 after all pieces land.

## Decisions / gotchas

- A preset's store_true default (e.g. host) can't be turned off by a flag (no --no-host); edit the answers. Fine for now.
- probe-rs `run --chip` takes t.probe_chip (STM32G0B1RE); download/attach/cnt use s.chip (STM32G0B1RETx), as G0.
- Port G0's files verbatim, then parameterise. Never touch the J-Link 1366:0105 (Donguru). Nucleo ST-LINK on omarchy-m1 shared with G4/G5: ask via `tpm work say`. Nothing to GitHub. `git rm HANDOFF.md` before `tpm work ready`.

## G4 / G5 state (parent, 24 Oct 2026)

Both rebased onto template main cc1c30b, not landed. g0-led-sync (G4): API 0.3.0 (leds ww_indication, clock ww_timebase via bedrock_sync ClockServer), LED on TIM2_CH1 PA5. g0-pindiag (G5): PinDiag on 50 free pins + B1, `just pins` via `ww pins tui`. Second to land takes API 0.4.0; both add api_snapshots/..._0_2_0.ron. P5/P6/P7 start once they land.

## W4 planned API (evo-current, to be confirmed)

`tests/evolution.rs`: `wire_weaver_evolution::Check::new(env!("CARGO_MANIFEST_DIR")).require_current(true).assert();`. justfile: `api-save:` -> `WW_API_SAVE=1 cargo test -p <api> --test evolution`.
