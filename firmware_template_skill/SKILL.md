---
name: firmware-template
description: >
  Generate and set up new bare-metal Rust firmware projects for microcontrollers (STM32, RP2040/RP2350,
  nRF52/nRF91, ESP32) with embassy, stm32-hal2, the stm32-rs stm32XXxx-hal crates (stm32f4xx-hal, stm32h7xx-hal, ...)
  or a bare cortex-m-rt skeleton; defmt/RTT logging, WireWeaver device API (no_std API crate + server over USB or RTT),
  memory.x partitioning, embassy-boot A/B bootloader, settings store in flash (ww_settings), cnt event counters, backup-register
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
uv run scripts/bedrock_gen.py memory-x   --chip STM32G0B1RE --bootloader --settings
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
   - bootloader `--bootloader` (embassy-boot, stm32/rp/nrf only)
   - settings `--settings` [`--settings-pages N`]: a `SETTINGS` region at the end of flash (end of bank 2 on
     dual-bank parts; default 8 KiB, at least 2 erase pages) and `src/settings.rs`, a `ww_settings` store on
     embassy-stm32's `Flash` (`ww_ecc_flash::EccSafeFlash` on G0 / H7 so a power cut can't leave a word that faults
     every boot) with one setting, `label`; with `--wire-weaver` also `led_at_boot`, served as `device.label` and
     `device.settings`. The code
     is generated for `--framework embassy` on STM32 G0 / H7 and parts without flash ECC (F0-F4, F7), without
     `--bootloader`; elsewhere only the region is reserved and the script says why. Replaces `--config-page`
     (answer `config_page` is read as `settings`).
   - counters: **always ask these two questions**, one after the other, even when everything else is left at
     defaults:
     1. *Use `cnt` event counters?* → `--counters` (adds the `cnt` crate, `-Tcnt.x`, `CNT_RAM_BUFFER_SIZE_WORDS`,
        `cnt!` in the main loop and `DefaultHandler`). `--ram-counters N` sets the RAM buffer size [64 words].
     2. Only if yes: *Also use BKP counters that survive resets (backup registers)?* → `--bkp-counters auto`
        (or `tamp`/`rtc` to force the peripheral). STM32 only (not F1); for other chips say it is not supported and
        skip. BKP counters keep the backup domain from being reset at boot, see `references/stm32-notes.md`.
   - WireWeaver: **always ask these questions**, one after the other, even when everything else is left at defaults:
     1. *Expose a WireWeaver device API (RPC methods, properties, streams, host client generated from the same
        trait)?* → `--wire-weaver`. Needs `--framework embassy` on STM32, RP or nRF; for other combinations say it
        is not supported and skip the rest.
     2. Only if yes: *Which transport?* → `--ww-transport usb` (the MCU's own USB, needs `--log defmt` and a chip
        with a USB device peripheral: STM32 parts are checked in stm32-data, RP2040/RP235x, nRF52833/52840) or
        `rtt` (through the debug probe, no extra hardware, works next to defmt/rtt logging).
     3. *Name of the API crate?* → `--ww-api <name>` [`<project>_api`], snake_case. Its name + version identify
        the API on the wire.
     `--ww-src <path>` uses a local WireWeaver checkout (relative to the output directory) instead of git.
     With `--wire-weaver` the output directory is laid out differently, see *What gets generated*.
     4. *Also the host side (a client, CLI and Python package next to the firmware, justfile, CI, git hooks, README /
        AGENTS / FEATURES / CHANGELOG / LICENSE, logo)?* → `--host` (needs `--wire-weaver`); `--cli-name SHORT` names
        the command line and its env prefix [project name].
   - board preset `--board <preset>` (`scripts/data/boards/<preset>/preset.toml`, e.g. `nucleo_g0b1re`): answers
     for the board (chip, LED, probe, `--wire-weaver`, `--host`, ...) plus what only the board knows (clock tree,
     README board section, `docs/board/`, logo). Explicit flags override it. Ask which board first: a preset
     answers most of the questions above.
   - debug probe `--probe VID:PID` [the board preset's, else the family's on-board probe: ST-LINK 0483:374b, CMSIS-DAP
     2e8a:000c on RP, J-Link 1366:1015 on nRF]: the cargo runner and every recipe select the probe by it, the serial
     comes from `<CLI>_PROBE` in the environment or `.env`; a PC with several probes never gets an implicit one.
   - crash capture is on by default on Cortex-M (`bedrock_crash`: the HardFault and panic handlers save a record
     that survives the reset, the next boot logs it with the reset cause and a boot counter, `--wire-weaver` serves
     it as `device.last_reset`); `--no-crash-capture` leaves it out (panic-probe / panic-rtt-target /
     panic-halt as before). It takes `bedrock_crash` from `--bedrock` like `--build-info` does.
   - embassy crates `--embassy release|main` [release] (STM32 with `--framework embassy`): `release` takes the
     crates.io embassy and prints a commented-out `[patch.crates-io]` for embassy main; `main` takes embassy-rs/embassy
     main at the rev pinned in `scripts/data/versions.toml` (active patch block, code in main's API) for what isn't
     released yet: double-buffered USB bulk endpoints on USB v1 / v4 (with `--wire-weaver`) and the G0 flash
     prefetch inside embassy. Suggest `main` only when the user
     wants those; it moves the firmware off released crates until the next embassy release.
   - board LED pin `--led` (PB14 / PIN_25 / P0_13 / GPIO8 by default), RTC usage `--rtc`
   - STM32H7 with SMPS: `--supply-config <variant>` [+ `--smps-voltage V1_8|V2_5`] — the script **refuses** to
     generate without it; the value must come from the board schematic. Explain the options
     (see `references/stm32-notes.md`) and ask, never guess.
2. Run `chip-info` first for STM32 parts to confirm the part exists in stm32-data and show memories.
   **Read the chip's errata sheet** for the silicon revision on the board (`chip-info` lists `doc: errata_sheet:`
   for STM32, `new` prints it as a note; for STM32 also http://www.efton.sk/STM32/gotcha/; RP, nRF, ESP: see
   `references/errata.md`) for everything the firmware will rely on (peripherals, clocks, flash, DMA, low power,
   boot), and tell the user what matters. Quote errata from the document.
3. Run `new`. Show the user the memory table from the generated `README.md` and the printed notes.
4. `cd <name> && cargo build` (`cd <name>/firmware` with `--wire-weaver`; and `cd bootloader && cargo build` when
   generated). All supported
   combinations compile out of the box; if a build fails, fix the generated project **and** the script.
5. Point out `TODO` markers in the generated sources (clock tree, `mark_booted()` placement, SRAM enable
   bits, backup register write-access, the 48 MHz USB clock on STM32 parts without HSI48, OTG VBUS sensing).
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
| `--build-info` | embeds bedrock build info (needs `--log defmt` and a buildable `embedded_bedrock`; `--bedrock <path>` for a local checkout). Off by default; builds with `--bedrock git` too (its WireWeaver comes from git, patched to a local `--ww-src`). |
| `--nightly`, `--build-core`, `--panic-immediate-abort` | pin nightly / `build-std` for smaller binaries. Not required for embassy any more (executor 0.10 statically allocates tasks on stable). |
| `--no-flip-link` | disable stack-overflow protection linker |
| `--min-bootloader 24K` | bootloader region, rounded up to erase sectors; dev+release profiles of the bootloader are `opt-level = "z"` so it fits |
| `--main-ram AXISRAM` | which RAM becomes `RAM` (default: AXISRAM > RAM/SRAM > largest) |
| `--offline --flash-size 512K --ram-size 128K --erase-size 2K --rust-target ...` | STM32 without network/cached data |
| `--embassy main` | embassy-rs/embassy main at a pinned rev through `[patch.crates-io]` instead of crates.io (STM32, `--framework embassy`); see `references/frameworks.md#embassy-main` |
| `--dry-run` | list files + memory.x without writing |

## What gets generated

`Cargo.toml`, `build.rs`, `.cargo/config.toml` (probe-rs/espflash runner, `DEFMT_LOG`, `CNT_*`), `rust-toolchain.toml`,
`memory.x` (not for ESP), `src/main.rs`, `justfile` and `scripts/setup.py` (`just setup`: the PC's toolchain and
tools, root steps printed), `bedrock_fw.json` (template origin + answers), `AGENTS.md` + `CLAUDE.md`
(template upgrade instructions), optional `src/init.rs` (STM32 backup-domain reset / backup register enable),
optional `src/init_ram.rs` (enable + zero extra SRAM banks, RCC bits looked up in stm32-data), `README.md` with
memory table and MCU documentation links, `bootloader/` (own crate, own `memory.x` with swapped region names), and
with `--settings` `src/settings.rs` (`DeviceSettings` with `#[derive(ww_settings::Settings)]`, `SettingsStore`:
`load` at boot, `set` + `persist_if_due` for writes once a change settles, `persist` for a write before answering;
on G0 an NMI handler in `main.rs` that hands flash ECC errors to `ww_ecc_flash::on_nmi()`).

With `--wire-weaver` the output directory is a project root: a host workspace (`Cargo.toml`, `Cargo.lock`) over the
API crate, and the firmware as a standalone crate excluded from it (it builds for the MCU target):

```
<name>/
  bedrock_fw.json AGENTS.md CLAUDE.md README.md .gitignore justfile   # upgrade flow runs from here
  Cargo.toml Cargo.lock   host workspace; #[ww_api_root] finds its git dependencies through this lockfile
  <ww-api>/     #![no_std] API crate: #[ww_api_root] trait with blinky stubs (led_on / led_off), std + defmt features;
                tests/evolution.rs checks the API against api_snapshots/ (`just api-check`, `just api-save`)
  firmware/     the firmware as above (+ bootloader/), src/ww.rs: ServerState implementing the trait, ww_codegen!,
                transport tasks (USB: embassy-usb driver + wire_weaver_usb_embassy; RTT: ww_device::rtt channels)
  .githooks/pre-commit, scripts/hooks.py   evolution check before each commit (`just hooks`, chained after the
                hooks already in .git/hooks)
  <name>_client/  --host: typed async methods over the generated client, one Error, Target = Probe (RTT: chip,
                VID:PID from --probe, serial from <CLI>_PROBE, ELF) or Usb (VID:PID, serial); log and counters
                decoded with the ELF found by build id (cnt_core)
  <name>_cli/   --host: the --cli-name binary (clap, colour, completion with live probe serials or USB devices,
                --version with git SHA and build time); info, led, label, last-reset and the device subcommands
  <name>_py/    --host: PyO3 + maturin module `Device`, uv project, examples blink.py / device.py, pytest
                (`just test-py`)
```

`--host` adds the three host crates above (items the answers leave out, such as settings, counters, the log or
boot, leave their methods and subcommands out), `FEATURES.md`, `CHANGELOG.md`, `LICENSE`, `.github/workflows/` (CI: lint, test, firmware build,
evolution check; setup check), `scripts/rename.py` (rename the project after cloning), `docs/logo/` (Typst, rendered
when `typst` is on PATH) and with `--board` `docs/board/` (boardmap TOML and geometry, rendered when `boardmap` is on
PATH). `new` runs `cargo generate-lockfile` and prints the next steps (`just api-save` for the first snapshot,
`just hooks`).

The LED moves into `ServerState` and is switched by the API; the main loop only idles. For USB on STM32 the
peripheral, DP/DM pins and interrupt come from stm32-data (`USB` > `USB_DRD_FS` > `USB_OTG_FS` > `USB_OTG_HS` in
FS mode); the 48 MHz clock is HSI48 + CRS when the part has both, otherwise a TODO (HSE + PLL). For RTT the log
channel stays up channel 0 (defmt then goes through `rtt-target` instead of `defmt-rtt`), WireWeaver uses
`ww_up` / `ww_down`. WireWeaver crates come from git (`ww_device` is not on crates.io).

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
