---
name: firmware-template
description: >
  Generate and set up new bare-metal Rust firmware projects for microcontrollers (STM32, RP2040/RP2350,
  nRF52/nRF91, ESP32) with embassy, stm32-hal2, the stm32-rs stm32XXxx-hal crates (stm32f4xx-hal, stm32h7xx-hal, ...)
  or a bare cortex-m-rt skeleton; defmt/RTT logging,
  memory.x partitioning, embassy-boot A/B bootloader, config flash page, cnt event counters, backup-register
  counters, extra SRAM bank init, build-info embedding. Use when asked to create/scaffold/bootstrap an MCU
  project, produce a memory.x/linker layout, pick rust target/probe-rs chip for an MCU, set up an embassy-boot
  bootloader partition table, or produce a Hubris memory.toml for a chip. Also use to upgrade an existing firmware
  generated from this template (has bedrock_fw.json) to a newer template version.
---

# Firmware project template

Everything is driven by one Python script, `scripts/bedrock_gen.py`, run with [uv](https://docs.astral.sh/uv/)
(Python ≥ 3.11 and Jinja2 are declared inline in the script and pinned in `bedrock_gen.py.lock`; uv installs
them on first run). Network is only needed for STM32 parts, cached in `~/.cache/bedrock_gen`.

```
uv run scripts/bedrock_gen.py list-chips
uv run scripts/bedrock_gen.py chip-info  --chip STM32H725IG
uv run scripts/bedrock_gen.py memory-x   --chip STM32G0B1RE --bootloader --config-page
uv run scripts/bedrock_gen.py new <name> --chip <chip> [options]
uv run scripts/bedrock_gen.py hubris-memory --chip STM32H743ZI
uv run scripts/bedrock_gen.py new --answers <fw>/bedrock_fw.json --out <tmp>   # regenerate with recorded answers
uv run scripts/bedrock_gen.py check-answers <fw>/bedrock_fw.json             # options new since generation
uv run scripts/bedrock_gen.py compare --current <fw> --new <tmp> [--base <tmp-old>] [--diff]
```

Paths are relative to this skill directory; use the absolute path when invoking.

## Workflow

1. **Collect requirements** from the user (ask only what is missing; defaults in brackets):
   - chip (`STM32xxxxxx` exact part w/o package suffix, `rp2040`, `rp2350`, `rp2350b`, `rp2354`, `nrf52832/33/40`, `nrf9160/51`, `esp32`, `esp32c3/c6/s3/...`)
   - framework `--framework embassy|stm32-hal2|stm32xx-hal|bare` [embassy] (`stm32-hal2` = the stm32-hal2 crate,
     `stm32xx-hal` = the per-series stm32-rs crate: F0/F1/F3/F4/F7/G0/G4/H7/L0/L4)
   - logging `--log defmt|rtt|esp-println|none` [defmt]
   - bootloader `--bootloader` (embassy-boot, stm32/rp/nrf only), config page `--config-page`
   - counters: **always ask these two questions**, one after the other, even when everything else is left at
     defaults:
     1. *Use `cnt` event counters?* → `--counters` (adds the `cnt` crate, `-Tcnt.x`, `CNT_RAM_BUFFER_SIZE_WORDS`,
        `cnt!` in the main loop and `DefaultHandler`). `--ram-counters N` sets the RAM buffer size [64 words].
     2. Only if yes: *Also use BKP counters that survive resets (backup registers)?* → `--bkp-counters auto`
        (or `tamp`/`rtc` to force the peripheral). STM32 only (not F1); for other chips say it is not supported and
        skip. BKP counters keep the backup domain from being reset at boot, see `references/stm32-notes.md`.
   - board LED pin `--led` (PB14 / PIN_25 / P0_13 / GPIO8 by default), RTC usage `--rtc`
   - STM32H7 with SMPS: `--supply-config <variant>` [+ `--smps-voltage V1_8|V2_5`] — the script **refuses** to
     generate without it; the value must come from the board schematic. Explain the options
     (see `references/stm32-notes.md`) and ask, never guess.
2. Run `chip-info` first for STM32 parts to confirm the part exists in stm32-data and show memories.
3. Run `new`. Show the user the memory table from the generated `README.md` and the printed notes.
4. `cd <name> && cargo build` (and `cd bootloader && cargo build` when generated). All supported
   combinations compile out of the box; if a build fails, fix the generated project **and** the script.
5. Point out `TODO` markers in the generated sources (clock tree, `mark_booted()` placement, SRAM enable
   bits, backup register write-access).
6. Tell the user that `bedrock_fw.json` (template commit + all answers) and `AGENTS.md`/`CLAUDE.md` should be
   committed: they let an agent upgrade the firmware to future template versions.

## Upgrading an existing firmware

A generated project contains `bedrock_fw.json` (`template.commit`/`version`, `answers`, `upgrades`, `rejected`,
`nuances`) and an `AGENTS.md` with the full procedure — follow it (source: `scripts/templates/app/AGENTS.md.j2`).
In short: `git log <template.commit>..HEAD -- firmware_template_skill` + `CHANGELOG.md` entries newer than
`template.version` → `check-answers`, ask the user about new options → regenerate with `new --answers` into a temp
dir (and the old commit into another via `git worktree`, as 3-way base) → `compare` + read the diffs, matching changes
by meaning → report per logical upgrade, skipping `rejected` ones → apply **only after approval** → build → update
`bedrock_fw.json` (new template commit, answers, `upgrades` entry, `rejected`, `nuances`).
For firmware without `bedrock_fw.json` (pre-0.4.0) see the 0.4.0 upgrade notes in `CHANGELOG.md`.

## Options you should know

| option | effect |
|---|---|
| `--build-info` | embeds bedrock build info (needs `--log defmt` and a buildable `embedded_bedrock`; `--bedrock <path>` for a local checkout). Off by default since `bedrock_build_info` depends on unpublished crates. |
| `--nightly`, `--build-core`, `--panic-immediate-abort` | pin nightly / `build-std` for smaller binaries. Not required for embassy any more (executor 0.10 statically allocates tasks on stable). |
| `--no-flip-link` | disable stack-overflow protection linker |
| `--min-bootloader 24K` | bootloader region, rounded up to erase sectors; dev+release profiles of the bootloader are `opt-level = "z"` so it fits |
| `--main-ram AXISRAM` | which RAM becomes `RAM` (default: AXISRAM > RAM/SRAM > largest) |
| `--offline --flash-size 512K --ram-size 128K --erase-size 2K --rust-target ...` | STM32 without network/cached data |
| `--dry-run` | list files + memory.x without writing |

## What gets generated

`Cargo.toml`, `build.rs`, `.cargo/config.toml` (probe-rs/espflash runner, `DEFMT_LOG`, `CNT_*`), `rust-toolchain.toml`,
`memory.x` (not for ESP), `src/main.rs`, `bedrock_fw.json` (template origin + answers), `AGENTS.md` + `CLAUDE.md`
(template upgrade instructions), optional `src/init.rs` (STM32 backup-domain reset / backup register enable),
optional `src/init_ram.rs` (enable + zero extra SRAM banks, RCC bits looked up in stm32-data), `README.md` with
memory table and MCU documentation links, and `bootloader/` (own crate, own `memory.x` with swapped region names).

Details: `references/memory-layout.md` (partitioning rules, symbols, embassy-boot offsets),
`references/stm32-notes.md` (SMPS, backup domain, TAMP/RTC registers, stm32-data JSON structure),
`references/frameworks.md` (crate versions, per-family quirks, `cnt` counters, Hubris).

## Maintaining the script

- Crate versions live in `scripts/data/versions.toml`; verify against crates.io before bumping
  (`curl -s https://crates.io/api/v1/crates/<name> -A x | jq .crate.max_stable_version`).
- Non-STM32 chips are in `scripts/data/chips.toml`; STM32 comes from `embassy-rs/stm32-data-generated`, plus
  the rust target / stm32-hal2 / stm32-rs HAL (`[xxhal.<series>]`) feature tables in `scripts/data/stm32.toml`.
- Generated file contents are Jinja2 templates in `scripts/templates/` (conventions in its `README.md`); the
  script only resolves the chip, computes the memory layout and picks templates.
- After changes, regenerate and build the matrix in `references/frameworks.md#tested-matrix`, and check that every
  generated `.rs` passes `rustfmt --edition 2024 --check`. Top-level single-line `use` runs are sorted by
  `sort_uses()` in the script, so Jinja chunks may emit them in any order; everything else (long lines, attribute
  arguments, nested `use` groups) must be written in rustfmt style in the template.
- Every change that alters generated output or options gets a `CHANGELOG.md` entry (bump the version) with
  **Upgrade notes** for existing firmware: upgrading agents rely on it. Commit it, since `bedrock_fw.json` records
  the commit hash (a dirty tree is flagged with `"dirty": true`).
- New `new` options are picked up automatically as answers (everything except `NON_ANSWERS` in the script); give
  them a sensible default and good `help`, `check-answers` shows it to users of older firmware. Renaming or removing
  an option breaks `--answers` for existing firmware: describe the mapping in the changelog.
