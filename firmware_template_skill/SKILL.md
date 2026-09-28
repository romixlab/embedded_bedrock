---
name: firmware-template
description: >
  Generate and set up new bare-metal Rust firmware projects for microcontrollers (STM32, RP2040/RP2350,
  nRF52/nRF91, ESP32) with embassy, stm32-hal2 or a bare cortex-m-rt skeleton; defmt/RTT logging,
  memory.x partitioning, embassy-boot A/B bootloader, config flash page, cnt event counters, backup-register
  counters, extra SRAM bank init, build-info embedding. Use when asked to create/scaffold/bootstrap an MCU
  project, produce a memory.x/linker layout, pick rust target/probe-rs chip for an MCU, set up an embassy-boot
  bootloader partition table, or produce a Hubris memory.toml for a chip.
---

# Firmware project template

Everything is driven by one dependency-free Python script: `scripts/bedrock_gen.py` (Python ≥ 3.9,
network only needed for STM32 parts, cached in `~/.cache/bedrock_gen`).

```
python3 scripts/bedrock_gen.py list-chips
python3 scripts/bedrock_gen.py chip-info  --chip STM32H725IG
python3 scripts/bedrock_gen.py memory-x   --chip STM32G0B1RE --bootloader --config-page
python3 scripts/bedrock_gen.py new <name> --chip <chip> [options]
python3 scripts/bedrock_gen.py hubris-memory --chip STM32H743ZI
```

Paths are relative to this skill directory; use the absolute path when invoking.

## Workflow

1. **Collect requirements** from the user (ask only what is missing; defaults in brackets):
   - chip (`STM32xxxxxx` exact part w/o package suffix, `rp2040`, `rp2350`, `rp2350b`, `rp2354`, `nrf52832/33/40`, `nrf9160/51`, `esp32`, `esp32c3/c6/s3/...`)
   - framework `--framework embassy|stm32-hal|bare` [embassy]
   - logging `--log defmt|rtt|esp-println|none` [defmt]
   - bootloader `--bootloader` (embassy-boot, stm32/rp/nrf only), config page `--config-page`
   - counters `--counters` (+ `--bkp-counters auto|tamp|rtc` on STM32 for reset-surviving counters)
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
`memory.x` (not for ESP), `src/main.rs`, optional `src/init.rs` (STM32 backup-domain reset / backup register enable),
optional `src/init_ram.rs` (enable + zero extra SRAM banks, RCC bits looked up in stm32-data), `README.md` with
memory table and MCU documentation links, and `bootloader/` (own crate, own `memory.x` with swapped region names).

Details: `references/memory-layout.md` (partitioning rules, symbols, embassy-boot offsets),
`references/stm32-notes.md` (SMPS, backup domain, TAMP/RTC registers, stm32-data JSON structure),
`references/frameworks.md` (crate versions, per-family quirks, Hubris).

## Maintaining the script

- Crate versions live in the `V` dict at the top; verify against crates.io before bumping
  (`curl -s https://crates.io/api/v1/crates/<name> -A x | jq .crate.max_stable_version`).
- Non-STM32 chips are in `BUILTIN`; STM32 comes entirely from `embassy-rs/stm32-data-generated`.
- After changes, regenerate and build the matrix in `references/frameworks.md#tested-matrix`.
