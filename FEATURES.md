# embedded_bedrock features and roadmap

This file is the single source of truth for what embedded_bedrock does, what is broken and what is planned, for
humans and AI agents alike. The changelogs ([template](firmware_template_skill/CHANGELOG.md), [crates](CHANGELOG.md))
record what changed and when; this file records the current state. It replaces the README checklists and holds the
device contract from the tpm project notes (P2528, from the vault page Baremetal/Base).

Last full review: 3 Oct 2026 (commit `0f45039`; template 0.5.0, crates 0.1.0). Template statuses come from its
changelog, SKILL.md and the tested matrix in `references/frameworks.md`; crate statuses from the code.

## How to use this file

- **Status** of each item:
  - ✅ done (template: generated projects build for the tested matrix)
  - 🚧 in progress or partially done (the note says what is missing)
  - 🧪 implemented and builds, not tested on hardware yet
  - 🐛 implemented, but with known bugs
  - ⬜ stub: crate or API exists but does nothing yet
  - 📋 planned
  - 💡 idea, not committed to
  - ⛔ blocked (the note says on what)
  - 🔍 probably done or obsolete, needs a check before closing
- **IDs** (`TPL-4`, `DEV-2`) are stable: never renumber or reuse one. New items take the next free number of their
  area. Use the ID in commit messages, CHANGELOG entries and code `TODO`s (`// TODO(BLD-3): ...`).
- Items are grouped by area. Each area lists what works first, then open items by priority.
- When finishing work, update the item in the same commit: mark it ✅, add a pointer (generator option, template,
  crate) and move it up to the done items of its area. Don't delete done items. Items that turn out obsolete go to
  [Dropped and superseded](#dropped-and-superseded) with a one-line reason.
- A bug you find but don't fix gets an entry (🐛 on the feature, or a new item) with what triggers it.
- Small code-level gaps stay as `TODO` comments; only those that limit users or block a feature get an item.

## Firmware template (`TPL`)

The generator in `firmware_template_skill/` (template 0.5.0).

- ✅ **TPL-1 Chips**: STM32 (all parts in stm32-data-generated), RP2040 / RP2350 / RP2350B / RP2354, nRF52832 /
  52833 / 52840, nRF9160 / 9151, ESP32 family. `list-chips`, `chip-info`.
- ✅ **TPL-2 Frameworks**: embassy (default), stm32-hal2, stm32-rs per-series HALs (F0 / F1 / F3 / F4 / F7 / G0 /
  G4 / H7 / L0 / L4), bare cortex-m-rt.
- ✅ **TPL-3 Memory layout from chip data**: `memory.x` with all RAM banks, extra SRAM banks enabled and zeroed
  (`init_ram.rs`), dual-bank flash with the DFU region in bank 2, OTP region ignored. `memory-x`,
  `hubris-memory` (Hubris `memory.toml`).
- ✅ **TPL-4 Bootloader**: embassy-boot A/B layout and a `bootloader/` crate (STM32, RP, nRF). Hardware test of an
  update cycle: see TPL-21.
- ✅ **TPL-5 Config page**: one erase sector reserved as `CONFIG` for persistent settings.
- ✅ **TPL-6 Logging**: defmt (explicit buffer size and level, optional non-blocking), RTT, esp-println, none.
- ✅ **TPL-7 Counters**: `cnt` RAM counters with `cnt!` in the main loop and `DefaultHandler`; backup-register
  counters that survive resets on STM32 (TAMP or RTC, not F1).
- ✅ **TPL-8 Size options**: pinned nightly, `build-std` core, `panic_immediate_abort`; flip-link on by default.
- ✅ **TPL-9 STM32 specifics**: SMPS supply config required (and checked) on H7 parts with SMPS pins; RTC quirk
  (backup domain not reset when `--rtc`); README with links to every datasheet and reference manual.
- 🧪 **TPL-10 WireWeaver device API** (0.5.0): `#![no_std]` API crate plus `src/ww.rs` server over USB (embassy-usb,
  STM32 / RP / nRF, HSI48 + CRS on STM32 where available) or RTT. Builds for the matrix; USB enumeration tested by
  hand only on the firmwares that adopted it.
- ✅ **TPL-11 Upgrades**: `bedrock_fw.json` records template commit, version and all answers; `new --answers`
  regenerates; `check-answers` lists new options; `compare` classifies files (take-new / merge / added / removed),
  3-way with a base. Generated `AGENTS.md` describes the report-first upgrade procedure.
- ✅ **TPL-12 rustfmt-clean output**: generated `.rs` files pass `rustfmt --edition 2024 --check` (0.4.3).
- 🐛 **TPL-13 USB clock TODO on parts without HSI48 / CRS**: F411, L476, F103 generate a WireWeaver USB project with a
  clock TODO instead of a working 48 MHz setup (tested matrix notes).
- 🐛 **TPL-14 Template `ww.rs` behind wire_weaver**: generated servers use the old `MessageSink` signatures; firmwares
  on the current wire_weaver checkout (request context for handlers, 3f1ccf8) had to adapt by hand (usb_io_fw,
  typec_dongle).
- 📋 **TPL-15 Relocate the vector table to SRAM**.
- 📋 **TPL-16 RTIC** as a framework option.
- 📋 **TPL-17 TODO notes in generated code**: on git dependencies (update periodically) and when the defmt buffer is
  small.
- 📋 **TPL-18 Board pin maps**: generate peripheral and pin assignments from a board description instead of only the
  LED pin.
- 📋 **TPL-19 Ozone and GDB / LLDB start scripts** in generated projects.
- 🔍 **TPL-20 Old cargo-generate template** (`bedrock_template/`): superseded by the skill; check nothing still uses
  it, then move it to *Dropped*.
- 📋 **TPL-21 Hardware test matrix**: bootloader update cycle, config page, BKP counters across reset, WireWeaver USB
  enumeration, on at least one STM32, RP and nRF board.

## Device contract (`DEV`)

What every device built on bedrock should offer, so host tools (RockFace, IOWeaver, CLIs) can treat all boards the
same. wire_weaver's `ww_stdlib` already has `ww_firmware_info`, `ww_board_info`, `ww_dfu`, `ww_counters` and
`ww_log_bare_metal` trait crates to build on.

- ✅ **DEV-1 Native USB interface** for the Rust API (WireWeaver over USB, TPL-10).
- 📋 **DEV-2 `fw_info` trait**: firmware name, version, git SHA, build time, in generated firmware.
- 📋 **DEV-3 `hw_info` trait**: PCBA name, revision, variant, serial.
- 📋 **DEV-4 `bootloader` trait**: status and entering the bootloader; the built-in ROM bootloader stays reachable
  through the BOOT0 button even when the firmware fails (PCB rule; the button can double as a user button).
- 📋 **DEV-5 Optional traits**: `embedded_log`, `counters`.
- 💡 **DEV-6 Update crate**: one Rust crate for checking and flashing devices, used by a CLI, a GUI and standalone
  (see zsa/zapp).
- 💡 **DEV-7 Web app from the device**: serve a jumpstart app from flash; load the full wasm app from the SD card,
  else the Internet, else a chosen file; version switching and diff loads from the browser cache.
- 💡 **DEV-8 USB and Ethernet devices**: serve the web app and a WebSocket server, give the host an IP from a unique
  10.x.y.z range. Decide whether a TCP server is needed next to WebSockets.
- 💡 **DEV-9 Ethernet discovery**: ZeroConf or similar announce, L2 access fallback (like MikroTik) when the IP is
  unknown.
- 💡 **DEV-10 Python API** forwarding to the Rust API (one source of truth).
- 💡 **DEV-11 Optional WireGuard interface**.

## Build info (`BLD`)

- 🚧 **BLD-1 Build info format** (`bedrock_build_info`): crate, target, compiler and version-control info,
  WireWeaver-serialized with a CRC; owned and borrowed forms. RockFace reads it from the ELF and shows it.
- 🚧 **BLD-2 Embedding** (`bedrock_build`): build-script helper that embeds the compact form and a defmt-interned
  full form. Off by default in the template (`--build-info`) because `bedrock_build_info` depends on unpublished
  crates.
- 📋 **BLD-3 Publish the crates** so `--build-info` can be on by default.
- 📋 **BLD-4 Flash SHA / CRC** embedded for quick comparisons, defmt lookup and the bootloader's image check.
- 📋 **BLD-5 Inject build info into the ELF after the build** (no source changes).
- 🚧 **BLD-6 RAM linking option**: `bedrock_build/link_ram_cortex_m.x` exists, not wired into the template.
- 📋 **BLD-7 Git hook** that replaces path dependencies with git links before pushing.

## Robustness (`ROB`)

- ✅ **ROB-1 flip-link** stack overflow protection (template default).
- ✅ **ROB-2 Fault handlers** with counters (`DefaultHandler`, `HardFault` with BKP counter).
- 📋 **ROB-3 HardFault reporting**: blink the address and flags in Morse code, then reboot (default) or keep blinking;
  keep the reason for the next boot.
- 📋 **ROB-4 UsageFault, MemManage, BusFault handlers** with clear messages.
- 📋 **ROB-5 Clock security**: CSS handler; enable LSE CSS; handle backup registers with an explicit backup-domain
  reset or BOR.
- 📋 **ROB-6 Stack usage estimate** (cortex-m-rt `paint-stack`; it caused a BusFault on B200, check before enabling).
- 📋 **ROB-7 Flash EEPROM emulation** with help from the bootloader: a swap page protects the latest config and the
  bootloader state against power loss during erase.
- 📋 **ROB-8 RTT doesn't block forever** when the probe disconnects (detect blocking mode).
- 💡 **ROB-9 Log to backup SRAM or the SD card**; defmt-brtt (RTT plus a ring buffer).
- 💡 **ROB-10 Counter arrays and tracing**: counter arrays, embedded counter names, Hubris-style counters, store time
  differences for tracing.

## Debug tool (`CLI`)

A scriptable command-line tool next to RockFace (the GUI). `bedrock/` has a probe-rs prototype; `bedrock_cli/` and
`fw_registry/` are stubs.

- ⬜ **CLI-1 `bedrock_cli`** and **`fw_registry`** crates: `println!("Hello, world!")` only. The root crate's
  `src/main.rs` is the same.
- 🚧 **CLI-2 Read build info from a connected target** (`bedrock/`, probe-rs).
- 📋 **CLI-3 Build, flash and upload to a local registry** for later defmt decoding by firmware SHA; link and run
  from RAM.
- 📋 **CLI-4 Connect to a running target with defmt**, fetching the ELF from the registry.
- 📋 **CLI-5 Target control**: reset, halt / go, attach GDB.
- 📋 **CLI-6 Diagnostics**: common pitfalls, bootloader state, stack usage, watchdog, RAM / flash ECC, flash CRC,
  flash and RAM usage, voltage and core temperature, HardFault analysis, configuration from flash.
- 📋 **CLI-7 Counters** display and reset.
- 📋 **CLI-8 Memory and registers**: read / write memory, watch an address or variable, registers with SVD decoding,
  where the PC points.
- 📋 **CLI-9 Peripherals**: GPIO status and manipulation, enabled peripherals and their configuration, clock tree
  with PLL frequencies, interrupts (enabled, priority, default handler, in RAM).
- 📋 **CLI-10 Install the tools** (flip-link, probe-rs, binutils).
- 💡 **CLI-11 Terminal to the firmware**, embassy CPU load, ETM.

## Testing support (`TST`)

- 📋 **TST-1 Host unit tests** in generated firmware (`#![cfg_attr(not(test), no_std)]`).
- 📋 **TST-2 On-target tests** with embedded-test (mcan's HIL crates are an example).
- 💡 **TST-3 Property tests** (`quickcheck`) and embedded-hal mocks.

## Docs (`DOC`)

- 🚧 **DOC-1 Book** (`book/`): common issues (defmt section missing, memory range errors); build info and counters
  pages are empty.
- ✅ **DOC-2 Skill docs**: `SKILL.md`, `references/` (frameworks and tested matrix, memory layout, STM32 notes).

## Dropped and superseded

(none yet)
