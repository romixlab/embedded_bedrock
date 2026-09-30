# Changelog

Changes to the firmware template (generator, templates, data) that matter for generated firmware. Newest first.
Generated projects record the version and git commit they came from in `bedrock_fw.json`; an upgrading agent
reads the entries newer than that version (see the generated `AGENTS.md`).

Each entry: what changed, and **Upgrade notes** — how an existing firmware picks it up, which answers/options are
involved, what needs a hardware test. Add an entry (bump the version) for every change that alters generated output
or options; purely internal refactors only need a line under the next version.

## [0.4.1] - 2026-09-30

- The backup-register region for `--bkp-counters` is named `BKPSRAM`, cnt's default `CNT_BKP_MEMORY_REGION`,
  so `.cargo/config.toml` no longer sets `CNT_BKP_MEMORY_REGION`. On chips that already have a real `BKPSRAM` RAM
  region (H7, H5, ... as listed by stm32-data) it stays `BKP_REGS` and the env var is still set.

Upgrade notes: only for `bkp_counters != "none"` on chips without a backup SRAM (e.g. G0, G4, L4). Rename `BKP_REGS` to
`BKPSRAM` in `memory.x` and drop `CNT_BKP_MEMORY_REGION` from `.cargo/config.toml`. Check with
`readelf -SW <elf> | grep cnt_bkp` that `.cnt_bkp_buffer` is still at the backup-register address.

## [0.4.0] - 2026-09-30

- `new` writes `bedrock_fw.json` (template repo/path/commit/version, all answers, upgrade history, rejected upgrades,
  firmware nuances) and `AGENTS.md` + `CLAUDE.md` describing the template upgrade procedure.
- `new --answers <bedrock_fw.json>` regenerates a project with the recorded answers; name and `--chip` become
  optional with it.
- New subcommands `check-answers` (options missing from / unknown to a firmware's answers) and `compare` (fuzzy,
  optionally 3-way comparison of a firmware with regenerated projects).

Upgrade notes: firmware generated before 0.4.0 has no `bedrock_fw.json`. Reconstruct the answers from the project
(`README.md` header, `Cargo.toml` features, `.cargo/config.toml`, `memory.x`), confirm them with the user, and find the
closest template commit by date (`git log --before=<project creation date>`); then follow the regular upgrade flow.
Take `AGENTS.md`, `CLAUDE.md`, `bedrock_fw.json` as new files.

## [0.3.0] - 2026-09-30

- `cnt` 0.4 (`4fb4c22`): `cnt!(name: u32)` counts unconditionally, `cnt_if!(cond, name: u32)`; BKP counters are placed
  by `cnt.x` into `BKP_REGS` via `CNT_BKP_MEMORY_REGION`; memory.x only declares the region. Generated code counts
  `led_toggles`/`loop_iterations`, `unhandled_exceptions`, with BKP counters `unhandled_exceptions_total`/`hard_faults`.

Upgrade notes: only for `counters: true`. Bump `cnt` in `Cargo.toml`, rewrite `cnt_if!(true, x: u32 += 1)` as
`cnt!(x: u32)`, add `CNT_BKP_MEMORY_REGION` to `.cargo/config.toml` and drop any hand-written `.cnt_bkp_buffer` section
from `memory.x`. Needs Rust 1.88+ and host `cnt_cli` 0.4 (rejects older firmware).

## [0.2.0] - 2026-09-28

- `--framework stm32xx-hal` (stm32-rs per-series HALs: F0/F1/F3/F4/F7/G0/G4/H7/L0/L4) (`1bd2751`).
- `--framework stm32-hal` renamed to `stm32-hal2` (`3cf6405`).
- Generator split into Jinja2 templates and TOML data tables, dependencies pinned via uv (`c23e644`).

Upgrade notes: answers with `framework: "stm32-hal"` must become `"stm32-hal2"` (same crate, same output).

## [0.1.0] - 2026-09-28

- First version of the skill (`01d6ae1`): STM32 / RP2040 / RP235x / nRF52 / nRF91 / ESP32 projects, embassy, stm32-hal
  or bare, defmt/RTT, embassy-boot bootloader, config page, cnt counters, backup-register counters, extra SRAM init,
  build info.
