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
- ✅ **TPL-5 Settings store** (was: config page, one `CONFIG` sector): `--settings` reserves a `SETTINGS` region of
  `--settings-pages` erase pages at the end of flash, at the end of bank 2 on dual-bank parts (default 8 KiB, at least
  2: 4 × 2 KiB on STM32G0B1), and generates `src/settings.rs`: a `ww_settings` store (WireWeaver STD-32) on
  embassy-stm32's `Flash`, read through `ww_ecc_flash::EccSafeFlash` on G0 / H7 (SRV-19, NMI handler on G0) or
  `BlockingAsync` on parts without flash ECC, with one setting (`label`), writes once a change settles and an erase
  ahead after each write. `--wire-weaver` serves it as `label: ww_user_info::UserInfo`, written before the call
  answers. Builds for the tested matrix. On Donguru B153A (16 Oct 2026, `--ww-transport rtt` through a J-Link,
  needs WireWeaver TR-13): label set over RTT, reloaded after a reset and after a power cut (USB unplugged). H7 and F4 built, not run. Code only for embassy on STM32 without `--bootloader` (TPL-24, TPL-25, TPL-26); elsewhere the
  region is reserved.
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
- ✅ **TPL-14 Template `ww.rs` on the current WireWeaver**: handlers take a request `Context`, `process_bytes` an
  `EventWriter` and the `Medium` (fixed after 0.5.0; usb_io_fw and typec_dongle had adapted by hand).
- 🐛 **TPL-13 USB clock TODO on parts without HSI48 / CRS**: F411, L476, F103 generate a WireWeaver USB project with a
  clock TODO instead of a working 48 MHz setup (tested matrix notes).
- 🐛 **TPL-22 Small output regressions** (seen in the 15 Oct 2026 matrix run): `rustfmt --check` fails for
  stm32xx-hal F3 / L4 (`let mut led = gpiob.pb14.into_push_pull_output(..)` too long) and G4 (`init_ram.rs` `count`
  line); embassy STM32 parts without SMPS or USB clock lines (G0, L4) warn `let mut config` doesn't need `mut`.
- 📋 **TPL-15 Relocate the vector table to SRAM**.
- 📋 **TPL-16 RTIC** as a framework option.
- 📋 **TPL-17 TODO notes in generated code**: on git dependencies (update periodically) and when the defmt buffer is
  small.
- 🧪 **TPL-18 Board hardware from the preset**: generate peripheral and pin assignments from a board description
  instead of only the LED pin. Done: `[[leds]]` with several mono LEDs and RGB, active level and role, each pin on a
  timer channel through `bedrock_led` (`leds_pwm()`, `templates/app/src/led_multi.rs.j2`; embassy STM32; matrix
  `g0leds`, `h7rgb`); `[[buttons]]` as `buttons` in the API (`src/ww/buttons.rs`), `[[init]]` pins, `[bring_up]` code
  with its dependencies, `[usb]` product, manufacturer, vid and pid (`preset_hw()`, `usb_init_with_ids` of wire_weaver
  TR-17; matrix `h7ulpi`). Next: typed host methods for `buttons`.
- 📋 **TPL-19 Ozone and GDB / LLDB start scripts** in generated projects.
- 🔍 **TPL-20 Old cargo-generate template** (`bedrock_template/`): superseded by the skill; check nothing still uses
  it, then move it to *Dropped*. Its manifests are `Cargo.toml.liquid`: cargo scans every `Cargo.toml` of a git
  dependency's repo, and the template tags in them gave parse errors in firmware taking `bedrock_crash` from git.
- 📋 **TPL-21 Hardware test matrix**: bootloader update cycle, settings stored across a power cut, BKP counters across reset, WireWeaver USB
  enumeration, on at least one STM32, RP and nRF board.
- 📋 **TPL-24 Settings store with `--bootloader`**: the embassy-boot updater in `main` and `src/settings.rs` both
  take the `FLASH` peripheral; share one `Flash` (blocking mutex, as the updater already uses) and generate the
  settings code for bootloader layouts too. Until then `--settings --bootloader` reserves the region only.
- 🚧 **TPL-25 Settings store on RP and nRF** (and the HAL frameworks): RP 🧪: `src/settings.rs` for embassy on
  RP2040 / RP2350 without `--bootloader` (`settings_flash()` "rp", `app/src/settings.rs.j2`): `ww_settings` on
  embassy-rp's blocking `Flash` through `BlockingAsync`, the region of `--settings-pages` sectors at the end of flash
  as usb_io_fw's b200_board does. An erase or write stalls XIP: embassy-rp runs it from RAM with interrupts off, on
  core 0 only, pauses core 1 when it was started with `spawn_core1`, and waits for DMA reading flash. Built in the
  matrix (`rp2040h`, `rp2350h`), not run on a board yet. nRF (embassy-nrf's NVMC) and the HAL frameworks: region only
  so far.
- 📋 **TPL-26 Settings store on other ECC flash** (G4, L4, L5, U5, WB, WL, H5, C0, U0): needs an `EccSafeFlash`
  family in WireWeaver's `ww_ecc_flash` first (`FLASH_ECCR` address and NMI flow per family); region only so far.
- 🐛 **TPL-23 `init_ram.rs` not rustfmt clean for long bank names**: `xf3` (F303VC), `xg4` (G474RE) and `xl4`
  (L476RG) of the tested matrix fail `rustfmt --edition 2024 --check`: the `let count = ...` line for banks like
  `CCMRAM_ICODE` is over 100 columns and rustfmt breaks it after `=`. Found 15 Oct 2026.
- ✅ **TPL-27 `just flash-bootsel` for RP targets**: a generated `justfile` (`app/justfile.j2`, `t.family == "rp"`
  only, both the plain and `--wire-weaver` layouts) with one recipe, `flash-bootsel [SERIAL]`: build release, then
  `ww dfu rp flash` (wire_weaver CLT-21) writes the ELF straight through PICOBOOT in the RP2040/RP2350 boot ROM
  (BOOTSEL mode), no probe, picotool, elf2uf2 or objcopy step; `SERIAL` waits for the device to (re)appear and
  come back. `references/frameworks.md`'s RP2040/RP235x alternative-runner line and both app/ww `README.md.j2`
  mention it. probe-rs stays the default `cargo run`/`cargo build` runner in `.cargo/config.toml`, untouched.
  Verified by generating an rp2040, an rp2350b `--wire-weaver` and an STM32 project; the `justfile` now also carries
  the `cnt` recipe (TPL-35) on every non-ESP family, so it is no longer RP-only.
- ✅ **TPL-35 `cnt run` as an alternative runner**: `.cargo/config.toml` (`app/cargo_config.toml.j2`, every non-ESP
  family) carries a commented `runner = "cnt run --chip <CHIP>"` under the active `probe-rs run` one, and the
  generated `justfile` gets a `cnt` recipe (`cargo build && cnt run`): both flash, reset and then show the defmt log
  plus live counters as they change (`cnt_cli`), without switching the project's default runner. `cnt` bumped to
  0.5 in `scripts/data/versions.toml` (macro API and the on-target buffer format, `SIGNATURE_VERSION`, are
  unchanged from 0.4; only the host `cnt_cli` version moves). Verified by regenerating and building `g0`, `g0rtt`,
  `rp2040`, `xf1` (stm32xx-hal) and `espc3` (no `justfile`, ESP keeps `espflash`/probe-rs only) of the tested matrix,
  plus their bootloaders where present.
- ✅ **TPL-44 Tested matrix as a script**: `references/frameworks.md#tested-matrix` is now data,
  `scripts/data/matrix.toml` (name, args, builds, tags: `ww`/`host`/`git`/`rtt`), and a `matrix` subcommand
  (`[NAMES|--tag T] [--jobs N] [--keep] [--bedrock PATH --ww-src PATH]`): generates each entry into a scratch
  directory, `cargo build`s the listed builds (`dev`/`release`/`bootloader`/`bootloader-nodefmt`; a `host` tag hook
  for the g0-gen-suite plan's `--host` option runs `cargo test`+`clippy` at the generated workspace root instead, no
  entry uses it yet), `rustfmt --edition 2024 --check` on every generated `.rs`, and the flip-link check
  (`_stack_start` == `.data`'s address) on every Cortex-M ELF, with a progress line per entry and a summary table
  with timings; exits 1 on a failure. `--bedrock`/`--ww-src` go to every entry except ones tagged `git` (`g0bigit`,
  which must keep the real git sources); `git`-tagged `--wire-weaver` entries get `cargo generate-lockfile` in their
  API crate first (TPL-43). `frameworks.md`, `SKILL.md` and `AGENTS.md` point at it instead of a hand-run block. Run
  against the whole matrix: the only failures are the known bugs TPL-22, TPL-23 and TPL-34.

## Device contract (`DEV`)

What every device built on bedrock should offer, so host tools (RockFace, IOWeaver, CLIs) can treat all boards the
same. WireWeaver's `ww_stdlib` already has `ww_firmware_info`, `ww_board_info`, `ww_dfu`, `ww_counters` and
`ww_log_bare_metal` trait crates to build on.

- 🚧 **TPL-28 embassy main for STM32** (`--embassy release|main`, default release): `main` takes every embassy crate
  from embassy-rs/embassy main at `embassy-main` in `scripts/data/versions.toml` through `[patch.crates-io]` (app and
  bootloader `Cargo.toml`, only the crates the project uses; `macros.j2` `embassy_patch`, `embassy_crates()`), with
  main's API in the code (`mux::Usbsel::Hsi48`, interrupt binding last in `usb::Driver::new` / `new_fs`); `release`
  prints the same block commented out. With `--wire-weaver` on a USB v1 / v4 part (`u.double_buffer`), `src/ww.rs`
  sets up double-buffered bulk endpoints through wire_weaver_usb_embassy's `usb_init_with_endpoints` (needs
  WireWeaver with it: branch s/19oct2026-ww-usb-endpoints, to land first). Builds for the tested matrix's
  `--embassy main` entries; not run on a board through the template yet (Donguru runs the same endpoints).
- ✅ **TPL-32 G0 flash prefetch, ES0548 check**: with `--framework embassy` on G0, `main.rs` sets `FLASH_ACR.PRFTEN`
  after `embassy_stm32::init` (embassy 0.6 leaves it off; main does it itself). On dual-bank G0B1 / G0C1 `memory.x`
  gets `ASSERT(__etext <= <bank 2 base>)` (`es0548_bank2()`): ES0548 2.2.10, prefetch fails on branches across the
  banks, so code reaching bank 2 fails to link instead of HardFaulting (Donguru FW-38). Checked: the ASSERT stops the
  link with its message when tightened.
- 🐛 **TPL-34 `f103` (STM32F103C8, `--wire-weaver`) no longer fits 64 KiB**: the release build overflows FLASH by
  ~1.3 KiB against WireWeaver main of 19 Oct 2026, with the template of `main` (0d6bd3a, plus the `Second::milli`
  fix it needs to compile at all) and of this branch alike; WireWeaver grew. With `--embassy main` it is 5.5 KiB, so
  that matrix entry uses the 128 KiB F103CB; since the device suite (TPL-41) the F103C8 release is 26.7 KiB over and the F103CB dev build 27 KiB over (its release fits); a 64 KiB part needs the suite trimmed (log, introspection docs) by an option. Options: a smaller default `MAX_MESSAGE_LEN`, leaving introspection out
  on 64 KiB parts, or the tested part moves to F103CB.
- ✅ **TPL-43 `--wire-weaver` with `--ww-src git`: the API crate has no `Cargo.lock`**: fixed with a host workspace
  root over the API crate (`suite/Cargo.toml.j2`, the firmware excluded), as `nucleo_g0b1re_template` has; `new`
  runs `cargo generate-lockfile` there (`post_steps()`), so `#[ww_api_root]` (wire_weaver_core
  `transform/cargo_source.rs`) finds the git dependencies. Found building `g0bigit` (23 Oct 2026).
- 🧪 **TPL-36 `just setup` in every generated project**: `scripts/setup.py` (`app/setup.py.j2`, run with uv, stdlib
  only) sets up a fresh Arch, Ubuntu / Debian or Windows (winget) PC: build tools, rustup with the toolchain, targets
  and components of `rust-toolchain.toml` (Xtensa: espup's `esp`), probe-rs (its installer), and per answers
  flip-link, espup, espflash, cnt, ww; the probe's udev rule (probe-rs's rules file; checked by the probe's ids per
  family; probe-rs's rules file pinned to a commit) or the ST-LINK driver on Windows. Before it changes anything it
  prints its plan, naming every installer script it fetches and runs. Root steps are printed, `--root` runs them,
  `--check` reports what is missing, `--no-probe` leaves the probe out (CI), `--skip TOOL` one step. A root `justfile` with `setup` is now generated for every target
  (RP's `flash-bootsel` moves into it, `cd firmware` with `--wire-weaver`). Built first in nucleo_g0b1re_template
  (its TPL-10), run there in an Ubuntu 24.04 container; `ww` from git needs wire_weaver P2401#23oct-1313. No git
  hooks step: it comes with the hooks (TPL-37) and chains to the hooks a repository already has.
- 🧪 **TPL-37 Template suite: the host side next to the firmware** (`--host`, needs `--wire-weaver`; what
  nucleo_g0b1re_template proved): `suite_files()` / `suite_context()` and `scripts/templates/suite/`: a root `justfile`
  for building, flashing and checking all of it (probe-rs / cnt recipes with the probe, `api-check`, `hooks`, `logo`,
  `setup`), `.github/workflows/ci.yml` (lint, test, firmware and bootloader build, evolution check) and `setup.yml`,
  `scripts/rename.py`, README (board prose from the preset), AGENTS (layout and procedures before the upgrade
  section), FEATURES and CHANGELOG skeletons from the options, LICENSE, `docs/logo/` (Typst, board name and chip label;
  `typst` runs when on PATH) and `docs/board/` with `--board`. `--cli-name` names the CLI and its env prefix. The host
  crates (`suite/{client,cli,py}/`, from G0 cc1c30b): `<name>_client` (typed methods for the device items the answers
  have, one `Error`, `Probe` over RTT or `Usb` by VID:PID and serial), `<name>_cli` (clap, anstream colour, dynamic
  completion with live probe serials or USB devices, `--version` from `build.rs`; info, led, label, last-reset,
  firmware, board, uid, variant, counters, log -f, settings, boot, dfu, report), `<name>_py` (PyO3 + maturin, uv,
  examples, pytest). The G0 answers regenerate G0's host crates modulo wording and reflow; the matrix's `--host`
  entries pass cargo test, clippy and `just test-py`. Not run against a board through the template yet.
- 🧪 **TPL-41 Device suite in firmware and API**: every `--wire-weaver` firmware serves the device contract as
  nucleo_g0b1re_template (cc1c30b) does, with the items the answers allow (`suite_context()` `s.dev`): the API crate
  (`ww/api/src/lib.rs.j2`) has `device: Device` with `firmware`, `last_reset` (crash capture), `board`, `uid`,
  `label` and `settings` (`--settings`, `led_at_boot`), `counters`, `log` (`--log defmt`: ww_defmt_logger, RTT tee),
  `variant` (STM32 DBGMCU_IDCODE + FLASH_SIZE, revisions from `stm32.toml` `[variant.<dev_id>]`; RP: TPL-55), `boot`
  and `dfu.node` (bedrock_rom_boot on G0 / single-core H7 without `--bootloader`, needs `--log defmt`; RP: TPL-56); the
  firmware
  `src/ww.rs`, `src/ww/device.rs`, `src/log.rs`, `src/boot.rs`; the boot word is taken out of cnt's BKPSRAM
  (`memory.x`, `CNT_BKP_BUFFER_SIZE_WORDS`). The G0 answers with `--board nucleo_g0b1re` regenerate G0's
  `firmware/src` byte for byte (that firmware runs on the board). Builds for the tested matrix's `--wire-weaver`
  entries but the 64 KiB F103C8 (TPL-34); parts under 64 KiB RAM get a quarter of the log buffers. The H7, RP and
  nRF code (uid, variant, rom_boot on H7) has not run on a board yet.
- 🧪 **TPL-53 CAN: FDCAN channels as `can[]`** (`--can board|LIST|none`, default none; `resolve_can()`): embassy
  STM32G0 / H7 with `--wire-weaver`, `--log defmt`, `--counters`; ported from B135 (`b135_fw/src/{can.rs,ww/can.rs}`,
  `b135_client/src/can.rs`). Channels from the preset's `[[can]]` (`instance`, `tx`, `rx`, `transceiver`,
  `standby`, `standby_active`, `fdcansel`) or a list `FDCAN1:PD1,PB8,stby=PC13,FDCAN3:PG9,PD12`, checked in
  stm32-data. Firmware `src/can.rs` (`app/src/can.rs.j2`: mcan from git or `--mcan-src`, a CAN task on an
  `InterruptExecutor`, queues and transmit credit per channel, bus-off recovery, standby pin held inactive; H7 kernel
  clock PLL2_Q 80 MHz unless `fdcansel`, two instances at most on `Controller::h7_half`) and `src/ww/can.rs`
  (`ww_can_bus::Can`: the six required items plus set_mode, recover, tx_abort, protocol_status, set_filters), the
  HAL-free logic in `bedrock_can` (DEV-17). With `--host` the client's `can` module, CLI `can status|configure|send|dump`,
  Python `can_*`, `examples/can_loopback.py` and README "CAN" with an internal-loopback test. Matrix run 28 Oct 2026:
  `g0can` (STM32G0B1CE, FDCAN1 PD1 / PB8, standby PC13) and `h7can` (STM32H725IG, FDCAN1 + FDCAN3) pass every step
  (dev, release, flip-link, host test, clippy, fmt, Python); non-CAN output unchanged. Not yet: a run on hardware
  (the loopback test first), G0 filter elements (mcan has none on G0: only the global filter), ES0491 2.22.3 (DAR
  transmission lost on arbitration, the `one_shot` option) not worked around, other families, the optional
  `ww_can_bus` items (tx_events, bus_errors, bus_load, transceiver, termination, ...).
- 🧪 **TPL-54 USB high speed through a ULPI PHY on H7**: `--usb-phy ulpi` (default `fs`) puts WireWeaver's USB on
  `USB_OTG_HS` with an external ULPI PHY: `stm32_usb()` takes its 12 pins from stm32-data (the preset's `[usb] ulpi =
  { NXT = "PH4" }` where there are several), `src/ww.rs` calls embassy-stm32 0.6 `usb::Driver::new_hs_ulpi` with
  512-byte bulk packets and `UsbTimings::hs_higher_speed()`; the preset's `[clock] usb = true` says its snippet sets
  the USB kernel clock (PLL3Q 48 MHz, which embassy checks also with a ULPI PHY), else HSI48 + CRS as for FS. Embassy
  STM32 release only (`--embassy main` refuses). Matrix `h7ulpi` (H725, NXT on PH4, HSE bypass + PLL3Q preset) builds;
  ES0491 Rev 9 has no device-mode OTG_HS erratum (`references/errata.md`). Not run on a board yet (Donguru).
- 🧪 **TPL-52 Silicon revisions that can grow**: the API's `SiliconRevision` (`ww/api/src/lib.rs.j2`) has `Unknown`
  first and the errata sheet's revisions in its order after it (`stm32.toml` `[variant.<dev_id>]` `revisions`, quoted
  from the sheet's Table 2 "Device variants"), `ww_repr = unib32`, catch-all `Newer(u32)`, the raw `rev_id` beside it:
  a revision added later is appended, an additive API change. The matrix's `api-evolve` build (`matrix_api_evolve()`)
  regenerates the API with one more revision and runs `ww api check`: at the end compatible, in front breaking. Needs a
  `ww` from wire_weaver 668483bd or newer (EVO-10, the checker knows the catch-all).
- 🧪 **TPL-55 Chip variant on RP**: `device.variant` of an RP2040 / RP2350 `--wire-weaver` firmware
  (`variant_context()` kind "rp", `RP_VARIANT` in `bedrock_gen.py`): `Variant { part, rev_id, rom_version, revision,
  package }` from SYSINFO CHIP_ID (PART, REVISION), the bootrom's version byte at 0x00000013 and, on RP2350, PACKAGE_SEL
  (`Package::A` QFN-60, `B` QFN-80; an RP2354 reads as its RP2350 package). The revision comes from the bootrom version,
  since CHIP_ID.REVISION can't tell all of them apart: RP2040 B0 / B1 / B2 = bootrom 1 / 2 / 3 (RP2040 datasheet 2.8),
  RP2350 A2 / A3 / A4 = bootrom 2 / 3 / 4 (RP2350 datasheet appendix C; A3 and A4 both REVISION 3).
  `SiliconRevision` grows by appending as TPL-52 (`from_rom_version`). With `--host` the client, CLI `variant` (`RP2350
  rev A4 (bootrom 4), QFN-60 (RP2350A)`), Python `variant()` and the README follow. Built in the matrix (`rp2040h`,
  `rp2350h`), not run on a board yet.
- 🧪 **TPL-56 Boot and update node on RP**: `device.boot` and `device.dfu.node` of an RP `--wire-weaver` firmware
  without `--bootloader` (needs `--log defmt`, `rp_boot()`): `reboot(Updater)` resets into BOOTSEL through
  `embassy_rp::rom_data::reset_to_usb_boot` (the RP2350 ROM's `reboot()` with a 10 ms delay, RP2350-E30), 50 ms after
  the reply (`src/boot.rs`, as usb_io_fw's b200_usb_io `reboot_task`); the state is always `Confirmed`, `confirm` is
  `Ok`, `revert` `Unsupported`, the node `Profile::InPlace` (bedrock_rom_boot's `dfu` answers, no family feature). The
  USB serial is `device.board`'s, which the RP2350 ROM reports in BOOTSEL too, so `ww dfu rp flash` (wire_weaver
  CLT-21) follows the board there. With `--host` the client, CLI `boot` / `dfu`, Python and README say BOOTSEL. Built
  in the matrix (`rp2040h`, `rp2350h`), not run on a board yet; whether the RP2040 ROM's serial matches (the flash
  chip's id) is not in its datasheet.
- 🧪 **TPL-57 The LED breathes on RP PWM**: `--led-mode auto` breathes on embassy RP too (`rp_led_pwm()`: every GPIO
  has a PWM slice channel, GPIO n is channel A when even, slice `(n >> 1) & 7` below GPIO 32 and `8 + ((n >> 1) & 3)`
  above on RP2350B, as embassy-rp's `impl_pin!` table). `src/led.rs` (`app/src/led-rp.rs.j2`, after usb_io_fw's
  `b200_usb_io/src/led.rs` and `b200_board/src/led.rs`): embassy-rp `Pwm` at about 2 kHz (TOP 0xffff) through
  `bedrock_led` mono, the same `led_task`, `led_at_boot`, `leds: ww_indication::Indication` and host side as TPL-39;
  the breath follows shared time with TPL-50. New option `--led-active high|low` (default high; `low` inverts the PWM
  output or the GPIO level, for an LED from 3V3 to the pin; RP only, TPL-67). Matrix: every embassy RP entry on GP25,
  `rp2350bled` (GP38, slice 11, active low), `rp2040gpio` (`--led-mode gpio --led-active low`). Matrix run 28 Oct 2026, all 12 RP entries pass. Not run on a board.
- 📋 **TPL-67 `--led-active low` outside RP**: the option refuses on STM32 (embassy `SimplePwm` /
  `ComplementaryPwm` output polarity, the HAL crates' GPIO), nRF and ESP; boards with an LED from 3V3 to the pin
  invert by hand there.
- 🧪 **TPL-39 The LED breathes on its timer channel** (`--led-mode auto|breath|gpio`, default auto): `led_pwm()`
  finds the LED pin's `TIMx_CHy` / `CHyN` and AF in stm32-data (`peripherals[].pins`; a plain channel before a
  complementary one, a general-purpose timer before TIM1 / TIM8) and names embassy's time driver off that timer
  (`time-driver-timN` from `TIME_DRIVERS`, never `-any`, which may take the LED's timer). `src/led.rs`
  (`app/src/led.rs.j2`): `SimplePwm` at 1 kHz, or `ComplementaryPwm` for CHyN, through embedded_bedrock's
  `bedrock_led` (mono, `embassy`), a 3 s breath by default, `led_on` / `led_off` hold it solid on / off, the
  `led_at_boot` setting starts it solid on. With `--wire-weaver`: `leds: ww_indication::Indication` in the API (mode,
  brightness, identify; `src/ww/leds.rs`), with `--host` the client's `leds_mode` / `brightness` / `identify`, the
  CLI's `leds show|mode|brightness|identify`, Python's `leds_mode`, `brightness`, `identify()` and a README "LED"
  section. A pin without a channel, `--framework` other than embassy and nRF / ESP (PWM not wired yet) keep the
  plain GPIO output (`new` says so); `--led-mode breath` refuses there. RP: TPL-57. The G0 answers with `--board nucleo_g0b1re`
  regenerate nucleo_g0b1re_template's FW-10 / API-4 leds part (v0.1.7, a182dbc); the clock is TPL-40. Builds
  for the tested matrix (`g0brd` PA5 TIM2_CH1, `g0ledn` PF1 TIM15_CH1N, `h7ledpb0` PB0 TIM3_CH3, `h7ledpe1` and
  `g0ledgpio` without a channel, `g0ledoff`, and every embassy STM32 entry on PB14; full run 24 Oct 2026: only the known TPL-22, TPL-23, TPL-34 fail); ran on a board only as G0's own
  firmware.
- 🧪 **TPL-40 Clock sync: `clock` steered by the host** (`--clock-sync auto|on|off`, default auto: with
  `--wire-weaver` on embassy STM32; `clock_sync()`): the firmware serves `clock: ww_timebase::Clock` through
  embedded_bedrock's `bedrock_sync` (`ClockServer`, U32 µs, no sync line), as nucleo_g0b1re_template FW-11 / API-4 /
  HOST-6 / PY-3 (v0.1.7, a182dbc). `src/sync.rs` holds the static shared clock (`Config` locks within 10 ms over
  RTT, bedrock_sync's 2 ms default over USB), `src/ww/clock.rs` the handlers (`clock.steer` async for its
  `time_map` point, `clock.info` names the time driver's timer), the LED's breath and blink follow shared time
  (`run_with`) where it breathes. API crate `ww_impl!(clock: ww_timebase::Clock)`; with `--host` the client's
  `src/sync.rs` (`clock_steering`, `wait_clock_locked`, `clock_check`, `clock_line`; `leds` beside it, `leds.rs`
  without the clock), the CLI's `clock [--watch N] [--timeout S]`, Python's `wait_clock_locked()` / `clock_check()`
  and `examples/clock.py`, README "LED and clock" (a preset's `readme-clock.md` adds what was measured on that
  board). The G0 answers with `--board nucleo_g0b1re` regenerate G0's clock parts byte for byte (firmware, API,
  client, CLI, Python, README). Matrix: `g0brd` (RTT), `g0usbh` (USB), `g0clkgpio` (clock without the LED's
  breath), `g0clkoff` (`--clock-sync off`), `l4clk` (Cortex-M4) and every embassy STM32 `--wire-weaver` entry.
  Matrix run 24 Oct 2026 (`--tag ww`, 48 entries): all pass but the known TPL-34 (f103 release); `g0brd` / `g0board`
  need `boardmap` on PATH for the pin table their `board_pins` test reads. Ran on a board only as G0's own firmware.
- ✅ **TPL-51 API evolution check requires the current version's snapshot** (wire_weaver EVO-12, 94a6b3a2):
  `tests/evolution.rs` calls `Check::require_current(true)` instead of `strict(true)` (TPL-38), failing until the
  version in the API crate's `Cargo.toml` has its own snapshot in `api_snapshots/` matching the source — the gap
  that let nucleo_g0b1re_template 0.1.5 ship API 0.2.0 without `..._0_2_0.ron`. `just api-save` now saves (or
  refreshes) the current version's snapshot by running the same test with `WW_API_SAVE=1`
  (`cargo test -p <api> --test evolution --config "env.WW_API_SAVE.value='1'"`, cross-shell on Windows too)
  instead of shelling out to `ww api save`, so saving needs no `ww` install. README "Changing the API", AGENTS.md,
  `scripts/rename.py` and the pre-commit hook message updated to match. Matrix run 25 Oct 2026 (`--tag ww`, 48
  entries): all pass but the known TPL-34; a scratch project's deliberate API change without a new snapshot was
  confirmed to fail `.githooks/pre-commit`. The matrix itself regressed after this landed (every `--host`
  `--wire-weaver` entry failed `host-test` on a fresh project's missing snapshot): fixed, see TPL-65.
- ✅ **TPL-65 Matrix regression from TPL-51's `require_current`**: every `--host` `--wire-weaver` matrix entry
  (g0brd, g0board and the rest) failed `host-test` on a freshly generated project with no API snapshot, on a PC
  without `ww` on PATH — `new`'s `post_steps` already runs `ww api save` right after generation when `ww` is there,
  so the gap only showed up where it is missing. `matrix_run_entry` now runs `just api-save` first for such an
  entry regardless, as `new` already tells a user without `ww` to; it only refreshes an existing snapshot where
  `ww` already made one, so this is harmless on either kind of PC.
- ✅ **TPL-66 `sort_uses` ordered two brace-group `use` lines of one path unlike rustfmt**: two `use a::{...}` lines
  of the same path compared equal in `sort_uses`, so it kept the templates' emission order; rustfmt orders them by
  their items. Seen on a board preset's clock (`use embassy_stm32::rcc::{Hse, ...}` next to
  `use embassy_stm32::rcc::{Hsi48Config, mux}`) failing the matrix's `cargo fmt --check` (found while building the
  B129A CANnify preset, TPL-58).
- ✅ **TPL-68 `can_task` signature unlike rustfmt without a standby pin on G0**: `src/can.rs` always wrapped the
  task's parameters, but with no `stby=` pin and no H7 layout they fit in 100 columns and rustfmt keeps them on one
  line (`--can FDCAN1:PD1,PB8` failed `rustfmt --check`). Fixed in `templates/app/src/can.rs.j2`; matrix `g0canhw`.
- 🧪 **TPL-50 Clock sync on RP and nRF**: RP done, nRF open. `clock_sync()` turns `clock` on with `--wire-weaver`
  on embassy RP too: the same `src/sync.rs` / `src/ww/clock.rs` as TPL-40 (`bedrock_sync`'s `embassy` feature is
  chip-independent), `clock.info` names embassy-rp's time driver timer (`TIMER` on RP2040, `TIMER0` on RP235x,
  embassy-rp `time_driver.rs`), bedrock_sync's 2 ms lock tolerance over USB; the LED breathes on shared time (TPL-57).
  Matrix: every RP `--wire-weaver` entry (`rp2040h`, `rp2350h`, `rp2040pins`, `rp2350bled`, `rp2040gpio` without
  the breath), `rp2350clkoff` (`--clock-sync off`). Not run on a board: USB round trips on embassy-rp unmeasured.
  nRF: `--clock-sync on` still refuses (embassy-nrf's RTC time driver, its source name and a board check).
- 📋 **TPL-47 `device.boot` / `dfu` without defmt**: `rom_boot_family()` leaves boot out unless `--log defmt`
  (`src/boot.rs` logs defmt values); `--log rtt` / `none` firmware on G0 / H7 could have it too.
- 🧪 **TPL-46 `--pins`: `ww_pin_diag::PinDiag` beside the device suite**: `--pins board|LIST|none` (`resolve_pins()`;
  the `nucleo_g0b1re` preset answers `board`) serves `pins` in the API as nucleo_g0b1re_template FW-12 / TPL-11
  (467f889) does: `board` takes every pin the preset's boardmap TOML marks free (no `[pins]` role or role `free`,
  the same rule as boardmap's pin table), minus the LED, USB DP / DM and SWD, named as there, plus the preset's
  `[pins.read_only]` (role `shared` only); without a preset an explicit list `PA0,PB3,PC13:ro`, checked against
  stm32-data's package pins. Firmware `src/pins.rs` (analog until a host sets or reads a pin, `release_all` on a
  link drop), `ww.rs` / `ww/device.rs` / `main.rs` parts, the API's `ww_impl!(pins: ...)`; host side `just pins`
  (`ww pins tui`, over RTT with `--rtt CHIP --probe`, over USB `--api`, with the board map as preset when there is
  one), README "Pins" (the preset's `readme-pins.md` adds the board part and a jumper test), the bare board's pin
  test plan in `docs/board/`, and with a pin table the client test `tests/board_pins.rs` that checks the list
  against it. The G0 answers with `--board nucleo_g0b1re` regenerate G0's `firmware/src`, `firmware/Cargo.toml`,
  the client test, the pin test plan and the README section byte for byte (that firmware passed `ww pins test` on
  the board); matrix entries `g0board`, `g0pinsh` (list, USB, `--host`), `g0pinsrtt` (list, no `--host`) and
  `h7pins` build. Embassy on STM32 with `--log defmt` only; the H7 code has not run on a board.
- 🧪 **TPL-48 `--pins` on RP and nRF**: RP done, nRF open. `resolve_pins()` takes RP pin names `GP0`..`GP29`
  (`GP47` on RP2350B), never the LED; SWD and USB have pins of their own. `src/pins.rs` (`app/src/pins-rp.rs.j2`) on
  embassy-rp's `Flex`: "analog" is the disconnected state (input buffer off, no pulls, output off; `Pins::new`
  disconnects every pin, since `Flex::new` turns the input buffer on), weak drive 2 mA, normal 4 mA. RP2350-E9
  (RP2350 datasheet appendix D.5.1, "Increased leakage current on Bank 0 GPIO when pad input is enabled", affects A2,
  fixed in A3: an input-enabled pad holds itself at about 2.2 V against the pull-down) is handled on every RP235x as usb_io_fw's `b200_board/src/pin_diag.rs` does: a
  pulled-down input keeps the input buffer off and a read turns it on, waits, reads and turns it off again. A board
  preset's map defaults its pin nets to `^GP\d+$` on RP (none exists yet). Matrix: `rp2040pins`, `rp2350bled`
  (pins above GP29). Not run on a board. nRF: `resolve_pins()` still dies (embassy-nrf's `Flex`, its pin names).
- 📋 **TPL-49 `--pins` without embassy**: the STM32 HAL crates (`stm32-hal2`, `stm32xx-hal`) and bare firmware have no
  `pins.rs` yet (it is written against embassy-stm32's `Flex`); `--wire-weaver` there serves the device suite only.
- ✅ **TPL-38 API evolution check**: every `--wire-weaver` API crate gets `tests/evolution.rs`
  (`wire_weaver_evolution`, require_current since TPL-51: the API's current version needs its own snapshot),
  `just api-check` / `just api-save`, the CI step, and `.githooks/pre-commit` installed by `scripts/hooks.py`
  (`just hooks`, also a `just setup` step): chained after the hooks already in `.git/hooks/`, never
  `core.hooksPath`. The first snapshot is wire_weaver's format, so `new` says to run `just api-save`.
- ✅ **TPL-45 Board presets and the probe by VID:PID**: `--board PRESET` (`scripts/data/boards/<preset>/`:
  `preset.toml` with `[answers]`, `[board]`, `[readme]`, `[logo]`, `[clock]` (a Rust snippet for `main.rs`),
  `[boardmap]`, plus the README board sections and the boardmap TOML and geometry for `docs/board/`); answers layer
  parser defaults < preset < `--answers` < flags. First preset `nucleo_g0b1re` (the boardmap example moved here from
  `bedrock_board/examples/`). `--probe VID:PID` (default the preset's, else the family's on-board probe) goes into the
  cargo runner, every probe-rs / cnt recipe and setup.py's udev check; the serial from `<CLI>_PROBE` (env or `.env`).
  A preset's store_true answer can't be turned off by a flag: edit the answers.
- 🧪 **TPL-58 Board preset `b129a_cannify`**: B129A CANnify Micro HV Pico-Lock (STM32G0B1CET6 LQFP48, rev A) in
  `scripts/data/boards/b129a_cannify/`: answers (WireWeaver over its own USB-C, settings, counters, backup counters,
  host crates, CLI `cannify`, `--pins board`, probe `0483:374b` for an ST-LINK on J2), the 12 MHz HSE through the PLL
  to 64 MHz with the FDCAN kernel on the HSE (as mcan's `hil/b129`), the LED D14 on PB14 (TIM15_CH1 breathe), ROM
  boot over J3, the board map from the KiCad board (`boardmap extract`, MAP-10 pin names) and README board sections.
  `[board] maker` (the README's link text, was a fixed "ST") and `[readme] reset_how` are new preset keys; board
  README pieces take `{api}`. Matrix entry `g0cannify`; generated as cannify_micro_template (public,
  romixlab/cannify_micro_template). Open hardware notes (a hobby board: landed without them, not a blocker): which
  ST-LINK sits on J2 (VID:PID), flash, the README's device steps, ROM boot over J3, the pin test of a bare board
  (`--save`), the ES0548 read for HSE / PLL / USB on the silicon revision fitted. CAN (FDCAN1 TX PD1, RX PB8, STB
  PC13 low) comes in with TPL-53 through a template upgrade of cannify_micro_template.
- 🧪 **TPL-60 Board preset `donguru_b153a`**: Donguru B153A (STM32G0B1CET6 LQFP48, rev A, Main variant: USB2512B
  hub) in `scripts/data/boards/donguru_b153a/`: answers (WireWeaver over the MCU's own USB behind the hub, settings,
  counters, backup counters, host crates, CLI `donguru`, `--pins board`, probe `1366:0101` for a J-Link on the
  receptacle's SBU), HSI16 through the PLL to 64 MHz (USB on HSI48 + CRS), the LEDs LD101 red PF0 (TIM14_CH1) and
  LD102 green PF1 (TIM15_CH1N) with the time driver on TIM4, PA4 high for the hub's SDA pull-up (`[[init]]`), the hub
  bring-up in `[bring_up]` (RESET_N on PC13 driven from RTC_OUT1 so it holds through a reset, as typec_dongle FW-46;
  usb251xb config over I2C2 PB10 / PB14 with port 2's D+ / D- swap; kept when RTC_CR shows it running), VID:PID
  c0de:cafe (wire_weaver's default), the analog PA7 button / slide switch documented (no digital button), ROM boot,
  the board map from the KiCad board (MAP-12) and README board sections. Matrix entry `g0donguru` (passes, with
  `g0cannify` and `g0board` unchanged); `donguru_template` generated from it (session folder for now, no remote yet):
  `just test`, `just lint`, the firmware build and `just board` pass. Probe: nothing attached on build-1 (29 Oct
  2026: no USB devices, no probe-rs), so the J-Link or second Donguru on the SBU is still to be found. Hardware (🧪,
  never through the Donguru J-Link): flash, the README's device steps, the hub kept through `boot reboot --updater`
  into the ROM's DFU, the pin test of a bare board (`--save`); tracked as P2528#29oct-1424.
- 🧪 **TPL-63 Board preset `pico2`**: Raspberry Pi Pico 2 (RP2350A, 4 MB W25Q32RV flash) in
  `scripts/data/boards/pico2/`: answers (WireWeaver over its own micro-USB, settings, counters, CLI `pico2`,
  `--pins board`, probe `2e8a:000c` for a Raspberry Pi Debug Probe on J2, the family default), `[pins]` taking GP23
  (SMPS mode), GP24 (VBUS sense), GP25 (the LED D2) and GP29 (VSYS/3 ADC) out, the RP2350-E9 pull-down workaround
  (generic since TPL-48, no preset code needed), BOOTSEL / PICOBOOT boot through TPL-56 (no `--bootloader`: that is
  embassy-boot's A/B scheme, a different thing). Raspberry Pi publishes no Pico 2 PCB/Allegro/KiCad file (board
  layout is officially the same as the original Pico/Pico H, raspberrypi.com Pico-series documentation): the board
  map geometry is Raspberry Pi's own public Pico (R3) Allegro file (P2624, imported with kicad-cli on omarchy-m1,
  GPIOn/GPIOn_ADCn nets renamed to GPn), J0's single 40-pad footprint split into J0L/J0R so the connector background
  (`draw_connector`) does not cover the whole board; electrical facts (pin functions, VBUS/SMPS/ADC wiring) come
  from the Pico 2 Datasheet RP-008299-DS-3 instead, cross-checked against it. Matrix entry `rp2350pico2`; generated
  as pico2_template (repo and GitHub visibility: Roman's call, publishing waits on his review). Three generator
  gaps fixed on the way, all RP-board-preset-generic (TPL-62's pico preset hits them too): a board preset on RP
  needs its own `[board] pin_nets` (boardmap's own default is STM32's `P[A-K]n`; `bedrock_gen.py`'s `--pins board`
  picks the RP pattern by chip family on its own, boardmap the Rust tool does not, so the pin table came out empty
  without it); `client/tests/board_pins.rs.j2`'s parser only matched `` | `P `` pin-table rows (STM32) and only
  split `NAME: CAPS` (STM32), not RP's `` | `G `` rows or `NAME = PIN_n: CAPS`; `suite/py/src/lib.rs.j2`'s `blink()`
  wrapper was always the multi-line form, which only happens to be what rustfmt wants for a long enough board
  accessor name (`pico2` is short enough that rustfmt collapses it to one line) — now a length check picks the
  right form. Open hardware notes (parked for Roman, `tpm work park`, never the Donguru J-Link): flashing over
  BOOTSEL and the Debug Probe, the README's device steps, the pin test of a bare board (`--save`), the SWD / debug
  connector's physical style (Pico 2's datasheet says it may differ from the R3 board the geometry is taken from).
- 🧪 **TPL-59 Board preset `b135b`**: B135B FDCAN ISO USB (STM32H725IGK6 TFBGA176+25, 1 MiB single bank, rev B) in
  `scripts/data/boards/b135b/`: answers (WireWeaver over USB high speed through the USB3315 ULPI PHY, `--usb-phy
  ulpi` with NXT on PC3, DirectSMPS, settings, counters, backup counters, host crates, CLI `b135`, `--pins board`,
  `--can board`, probe `1366:0101` for a J-Link on the Tag-Connect J202), the 24 MHz HSE bypass clock tree of
  vb135_fdcan_iso_usb_fw (PLL1 520 MHz at VOS0, PLL2 Q 80 MHz FDCAN, PLL3 Q 48 MHz USB), the RGB LED D201 lit low
  (PC7 / PC6 TIM3, PH13 TIM8_CH1N), the button SW201 read on PE1 (BOOT0), init pins (PH3 the PHY's reset high, PH5
  the HS / FS mux low, the termination relay's PC13 / PE4 low), `[[can]]` FDCAN1 PB9 / PB8 (ADM3050E) and FDCAN3
  PG9 / PD12 on J203, ROM boot through bedrock_rom_boot (H7) with DFU over J301 at full speed, the board map from the
  KiCad board (MAP-11) and README sections with the ES0491 errata that touch the board (2.2.5, 2.2.6, 2.22.1-3).
  The termination relay K501 is documented, not driven: `can[]` has no terminator yet. Matrix entry `h7b135b`;
  generated as b135_template (not published yet). Open hardware steps: flash through the J-Link, the README's device
  steps (USB HS enumeration, `info`, LEDs, SW201), ROM boot and DFU over J301, both CAN channels on a bus, the pin
  test of a bare board (`--save`), the silicon revision fitted (DBGMCU REV_ID) against ES0491.
- 🧪 **TPL-61 Board preset `report_b200a`**: B200A rePort (RP2350B QFN-80, 512 KiB BY25Q40, 12 MHz crystal, rev A)
  in `scripts/data/boards/report_b200a/`, facts from `hw/b200_re_port/doc/hw_b200a.md`: WireWeaver over its own
  USB-C (product `rePort`), D102 on GP38 active low breathing on PWM slice 11A (TPL-57), SW101 on GP22 as
  `buttons.sw101` (pressed high, internal pull-down, RP2350-E9 read), `[bring_up]` latching the 595 chain's safe
  word before its outputs are enabled (EXP.~OE has no pull-up on rev A; usb_io_fw's chain.rs is the model), settings
  in the last four 4 KiB sectors (0x7c000, as usb_io_fw's `b200_board/src/flash.rs`, TPL-25), boot and dfu through
  BOOTSEL (TPL-55, TPL-56; by hand: J103 pin 5 to pin 6 while plugging in), the shared clock (TPL-50), `--pins
  board` on the MCU sides of the translators, the SD lines and SYNC with GP1, GP26, GP30, GP33, GP36, GP39 read
  only (TPL-48), the board map from the KiCad 10 board (`boardmap extract --pin-names`, MAP-1 enclosure outline).
  `[bring_up] what` is a new preset key (the generated FEATURES row). Matrix entry `report`; generated as
  report_template. Open hardware steps: which probe sits on J101 (VID:PID; 2e8a:000c assumed), flash, the README's
  device steps, BOOTSEL by `boot reboot --updater` and by J103, the pin test of a bare board (`--save`), the
  stepping fitted (`variant`, BOM orders A4), the button with the E9 read. Not done: the RP2350's own regulator
  left on (VREG_LX unconnected; the datasheet's "approximately 400 µA until it's shut down" by VREG HIZ). B200B
  (mid-Nov 2026) gets a revision of the preset.
- 📋 **TPL-64 USB configured marks the boot healthy**: with `--ww-transport usb` and ROM boot, the generated
  firmware never calls `bedrock_rom_boot::usb_configured()`, so `healthy_task` always waits the full 10 s; call it
  when the USB device is configured (wire_weaver_usb_embassy's state).
- 🐛 **TPL-69 H7 firmware does not report a supply locked by the ROM bootloader**: ROM bootloader versions before
  V9.3 (AN2606) leave PWR_CR3 locked in its reset state (SMPS and LDO) when they jump to the application, and
  PWR_CR3 takes one write per power-on, so a `--supply-config DirectSMPS` firmware started from the ROM (ROM boot,
  `boot reboot --updater`, TPL-41) runs on SMPS + LDO until a power cycle without saying so (embassy-stm32 does not
  check the write). Harmless where VDDLDO, VCAP and VFBSMPS share a net (B135B), wrong elsewhere. Read PWR_CR3 back
  before `embassy_stm32::init`, pass embassy what it holds, log and count the mismatch, as vb135_fdcan_iso_usb_fw's
  `b135_fw/src/clocks.rs` `supply_config()` does. Found on the B135B preset (TPL-59).
- 📋 **TPL-29 embassy main for RP, nRF and ESP**: `--embassy main` dies for them (`cmd_new`); main renamed RP API
  too (`pads::vals::Drive::_8mA`, `embassy_rp::mode::Blocking`, seen porting rePort, usb_io_fw).
- ✅ **TPL-33 Errata note**: `new` prints where the chip's errata sheet is (`errata_note()`: STM32 from stm32-data's
  `errata_sheet` doc plus efton.sk's STM32 gotchas, a pointer for RP / nRF / ESP), also in the generated
  `README.md`; `references/errata.md` lists where per vendor and the errata the template handles; SKILL.md
  workflow step 2 and AGENTS.md *Conventions* say to read it.

- ✅ **DEV-1 Native USB interface** for the Rust API (WireWeaver over USB, TPL-10).
- 🧪 **DEV-2 `fw_info` trait**: `--wire-weaver` firmware serves `ww_firmware_info::FirmwareInfo` (name, version, git
  commit and dirty flag, build time, profile, GNU build id) from `<Root>Consts`; not tried against a host yet.
- 🧪 **DEV-12 Status LED crate** (`bedrock_led`): the breath / blink animation with eased fades (smoothstep, exact
  and monotonic in 32-bit multiplies), gamma 2 and a perceptual peak, an identify blink, u32 math only (no 64-bit
  libcalls on the M0+), host-tested (`Animation`, `ease`, `scale`, `duty`); `Leds<N>`, N LEDs under one indication
  (mode each, visible each, one peak, a colour per RGB LED as per-channel peak, identify over everything; HAL-free,
  host-tested); `Channel` and `Pwm` over embedded-hal 1.0 `SetDutyCycle` (active-low option, `max_duty_cycle` read
  once); feature `embassy`: `Indication<N>` in a `static` (zero-initialised, `.bss`; one critical section per tick
  for all changes) and its `run` loop (10 ms ticker, sleeps while nothing animates); feature `ww-indication`:
  brightness peaks and the identify busy / cap of a `ww_indication::Indication` server. Used by Donguru (two mono
  LEDs, typec_dongle FW-2 / API-9, +32 B flash) and rePort (one, usb_io_fw PLAT-3, +100 B), the same compare values
  as before but for Donguru's old 32-bit ease (at most 6 of 64000 counts). rePort B200 flashed, `device.indication`
  mode, brightness and identify answered (21 Oct 2026, with the interim 64-bit ease, same curve); the RGB path not
  run on a board yet (B135 next). The template does not generate LED code with it yet.
- 🧪 **DEV-13 Shared clock crate** (`bedrock_sync`): the local clock disciplined onto UTC (wire_weaver STD-49 host
  sync, design P2528 `clock-sync.md`). `SharedClock` (HAL-free, host-tested) follows its best source (a locked line
  beats the host), slews to each new steer point at 1 % instead of stepping (steps only from free or beyond 1 s, with
  a `generation` that changes), coasts in holdover after 3 s without points and counts as free after an hour, with an
  error estimate that grows in holdover; an explicit source list (`Sources`, best first, `set_sources`: a source left
  out is only noted, `None` keeps the fixed ranking; `Source::Line(Line::IrigB | Pps)`); `local_at` for scheduled
  actions. Hot path (`shared`, every LED tick) without
  any libcall on the Cortex-M0+ at opt-level `z` and `3` (checked in the disassembly, 21 Oct 2026): 64-bit adds, 32-bit
  products of chunks up to 32 ms; a gap longer than that takes a cold path with two `__aeabi_lmul`, a steer point
  (once a second) a 64-bit divide. `Oscillator` backend: a chip with a fractional PLL takes the rate in hardware
  (1/64 of each point's rate, unquantised target, clamp without wind-up; tested against a host whose rate lags by its
  30 s window: a PLL 50 ppm slow converges without overshoot); first user B135B PLL1 FRACN (tpm P2404#21oct-1657),
  not Donguru's HSI trim (too coarse) nor the RP2350 (no fractional PLL). Feature `embassy`: `Sync` in a `static` on
  `Instant` (`shared_now`, `steer`, `wait_until_shared`); feature `ww-timebase`: `ClockServer`, the `Clock` server
  bookkeeping (`steer` reading 32-bit `local`, `sync_status`, `time_map_point`, `configure_sync`, `set_sources` /
  `sources`). States (21 Oct 2026): `Acquiring` while the slew left plus the source's error is above
  `Config::lock_within_us` (2 ms: a full-speed USB board's own error bound is about 1 ms), slewing at 5 %; `Locked`
  once within, slewing at 1 %; back to `Acquiring` beyond twice the setting or on a step; `SyncStatus::slew_us`.
  One steerer at a time (22 Oct 2026, wire_weaver SRV-31): of the host's points, the clock follows one steerer
  (`Steer::steerer`) while its points come within the holdover time, notes others (`Steered::OtherSteerer`), and
  lets the next one take over (slewed) once the followed one was quiet that long; `sync_status` names the followed
  and the last noted steerer (`SyncStatus::steerer` / `noted`, names cut to 24 B). Cost on Donguru (release,
  opt-level 3): flash +1552 B (text +1240, rodata +304, data +8), RAM +144 B; frames at opt-level 3 on thumbv6m:
  `ClockServer::steer` 88 -> 120 B, `SharedClock::steer_with` 104 -> 112 B, `sync_status` 104 -> 176 B. Hardware
  (omarchy-m1, 22 Oct 2026): `ww sync` locks the three boards as before (1.2-1.6 s); wire_weaver's
  `two_steerers` example on Donguru and B135: A followed, B noted (answered `Host`, `sync_status` names both) and
  still noted 10 s later, B followed and locked once A stopped (seen by B's host 6.5 s after, its checks come every
  5 s once locked; on the device 3 s quiet plus B's next point).
  Closed-loop test against wire_weaver's `Estimator` (`tests/closed_loop.rs`, simulated USB): a board 250 ms off is
  within 2 ms in 5 s (was 25 s), ticks never above 10.1 ms. Hardware (omarchy-m1, 21 Oct 2026, `ww sync` with Donguru B153A, rePort B200 and B135B): all three locked within 2 ms after 1.2 s from a fresh start and 2.4 s after 45 s in holdover; LED phase as ww sync estimates it 30 µs to 1.0 ms apart (±1.5-2.5 ms, rePort's and Donguru's ~1.4-1.8 ms round trips); over 5 min Donguru left once on a -2.0 ms residual (±1.2 ms) and was back in 37 s. No logic analyser on this PC: the phase is not measured on the pins.
- 🧪 **DEV-14 Status LEDs in phase** (`bedrock_led` with `bedrock_sync`): `Animation::align(SharedTime)` puts a breath
  or blink at `t mod period` of shared time (UTC), WSPR-style, so boards showing the same mode are in phase with
  nothing passed around (wire_weaver STD-50); `Leds::align` for all LEDs, `Indication::run_with(modes, shared, show)`
  reads shared time every tick (`run` unchanged: free-running from the last mode change, as before). Without shared
  time, or for Off / On and periods over 71 min, nothing changes; leaving alignment, the animation runs on free from
  where it was; the identify blink always runs free. Per tick 32-bit math only (one `__aeabi_uidiv` per LED, no 64-bit
  libcall, checked in the disassembly at opt-level `z` and `3`); the cycle start is found with one 64-bit remainder
  only after a mode change or a step of the clock (`generation`). `align` 180 B, `anchor_at` 52 B at `z`. Host
  tests: aligned breath and blink, two devices set at different times agree tick by tick, hand-over to free running,
  a new generation. Hardware check open: Donguru and rePort breathing in phase on omarchy-m1 (P2528 piece 6).
- 🧪 **DEV-15 Common look** (`bedrock_led::look`): what a status LED shows for what it means, named by meaning and
  the same on every board (`OK`, `BUSY`, `WARN`, `FAULT`, `ALERT`, `HEARTBEAT`, `IDENTIFY`, `OFF`; palette
  `colour::{OK, WARN, FAULT, STORAGE}`; `Look::under` for the `ww_indication` mode); every period divides the 3 s
  `CYCLE_MS`, so on shared time all looks of all boards start together. Used by Donguru (power `OK`, status
  `HEARTBEAT`), rePort (`OK`, was a 4 s breath) and B135 (SD `OK` in `STORAGE` / `FAULT`, CAN `OK` / `WARN` /
  `ALERT`). Kept on a module of `bedrock_led`: it holds `Mode`, the fades and the peaks already; a buzzer or display
  vocabulary would get its own module next to it when one exists. Flashed on all three boards (21 Oct 2026); the looks themselves not yet looked at by a person.
- 🧪 **DEV-16 STM32 ROM bootloader entry crate** (`bedrock_rom_boot`): firmware alone at the start of flash, updated
  by the STM32 ROM bootloader's USB DFU (`ww dfu stm32 flash`); extracted from Donguru (typec_dongle FW-12, FW-45,
  G0B1) and B135 (BOOT-4, BOOT-5, BOOT-9, BOOT-10, H725). `BootWord` in the family's last backup register (G0
  TAMP_BKP4R, H7 RTC_BKP31R; Donguru's format): ROM request, fell-back flag, unhealthy-boot count. `early()` first in
  `main`, before the HAL: resets once when the ROM's leave jumped here (`Entered::from_reset` per family, a second
  jump in a row goes on), takes a request and jumps to system memory (AN2606: G0 0x1FFF_0000, H72x-75x 0x1FF0_9800),
  else counts the boot and returns `Start::Rescue` after three boots without `mark_healthy`; `rescue()`, `reboot()`,
  `after_init()` (the H7's first init resets the backup domain), `usb_serial()` (the ROM's serial form, so a host
  follows the board). Feature `embassy`: `request_reboot` / `reboot_when_asked` (50 ms for the reply),
  `usb_configured` / `healthy_watch`; feature `ww-dfu`: `dfu::{boot_state, confirm, revert, reboot_target, node}`
  (`Profile::InPlace`, always `Confirmed`, last result `Recovery` after a rescue). Raw RM addresses, no HAL
  dependency (checked against stm32-metapac 21). Decisions host-tested; builds and links for thumbv6m (G0B1) and
  thumbv7em (H725) (`just lint`, a scratch firmware calling every function). Not run on a board through this crate
  yet: Donguru and B135 run the code it came from; first user nucleo_g0b1re_template's device suite (P2528#23oct-1258g).
  Not yet: H7A3/7B3 (other RCC offsets, ROM at 0x1FF0_A000), dual-core H745/H755, other families; B135's A/B
  bootloader path (feature `ab`) and its LED codes stay in B135 (`early_with` takes a closure for the code).
- 🚧 **DEV-17 FDCAN channel crate** (`bedrock_can`): an FDCAN (Bosch M_CAN) channel as `ww_can_bus::Can` without a
  HAL, ported from B135's `b135_board::can` (CAN-1) with the kernel clock and the message RAM layout as a
  `Controller` (`h7`: 64 / 32 filters, FIFOs 64 / 32, TX 32 as B135; `h7_half` for a second H7 instance; `g0`: the
  fixed 3 / 3 / 3 RAM and no filter elements, which the `mcan` driver lacks on G0 so far): bit timing from a rate and
  sample point (exact rates only), configuration checks (`resolve`), filters onto elements (`HwFilters::map`),
  frames to send (`tx_plan`), error state (`status`), capabilities with the board's terminators. Host-tested,
  no_std (clippy for thumbv6m). Used by the template's `--can` (TPL-53, matrix `g0can` / `h7can`) and its host client; B135 could move onto it
  (its firmware keeps `b135_board::can` until its owner decides).
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
  full form (`serialize_build_info`), or the compact form alone for a crate without defmt
  (`serialize_compact_build_info`: the bootloader without its `defmt` feature). Off by default in the template
  (`--build-info`); since BLD-8 it builds with `--bedrock git` too.
- 📋 **BLD-3 Publish the crates** so `--build-info` can be on by default.
- 📋 **BLD-4 Flash SHA / CRC** embedded for quick comparisons, defmt lookup and the bootloader's image check.
- 📋 **BLD-5 Inject build info into the ELF after the build** (no source changes).
- 🚧 **BLD-6 RAM linking option**: `bedrock_build/link_ram_cortex_m.x` exists, not wired into the template.
- ✅ **BLD-8 Builds from git**: the workspace takes WireWeaver by git (root `Cargo.toml`), local work takes the
  sibling `../wire_weaver` through the `[patch]` in `.cargo/config.toml`, which only builds run inside the checkout
  see. So firmware's `bedrock_build` and `cargo install --git ... bedrock_board_cli --locked` work on a PC without
  the sibling; generated firmware with a local `--ww-src` patches bedrock_build's WireWeaver to that checkout (one
  WireWeaver in the build). Tested matrix entries `g0bi` and `g0bigit`.

## Robustness (`ROB`)

- 🧪 **ROB-1 flip-link** stack overflow protection (template default, Cortex-M): `linker = "flip-link"` in
  `.cargo/config.toml` (ROB-12). The stack sits below `.data` / `.bss`, checked in the ELFs of the tested matrix;
  a deliberate overflow on a board not yet tried.
- ✅ **ROB-2 Fault handlers** with counters (`DefaultHandler`, `HardFault` with BKP counter).
- 🧪 **ROB-11 Crash capture** (PLAT-7 for every bedrock firmware): `bedrock_crash` keeps a fault / panic record in
  `.uninit` RAM across the reset (magic + CRC, never read after power loss), decodes the reset cause (STM32 `RCC_CSR` /
  `RCC_RSR`, RP `CHIP_RESET` + `WATCHDOG.REASON`) and counts boots; record logic tested on the host. Template: on by
  default on Cortex-M (`--no-crash-capture`), HardFault / panic handlers, `src/last_reset.rs`, served as
  `ww_firmware_info::LastReset` with `--wire-weaver`; builds for the tested matrix. Open: a forced HardFault on a
  Pico 2 and an H7 reported after reset and symbolized with the ELF (no probe on the mesh yet); the RP235x
  `CHIP_RESET` after `SYSRESETREQ` (stale flags?) and H7 ECC RAM behaviour checked there too; nRF `RESETREAS` and
  RISC-V (`Fault::RiscV`) not done.
- ✅ **ROB-12 flip-link was never used**: the template's `build.rs` printed `cargo:rustc-linker=flip-link`, which is
  not a build-script directive (Cargo ignored it), so generated firmware linked with plain rust-lld and `.data` /
  `.bss` sat at the start of RAM. Fixed 15 Oct 2026: `linker = "flip-link"` under `[target.<triple>]` in
  `.cargo/config.toml` (`templates/app/cargo_config.toml.j2`, application and bootloader). The extra-RAM sections in
  `memory.x` lost their tail `. = ALIGN(n)` so an empty one is dropped: flip-link 0.1.12 counts an empty section at
  the end of RAM (RP2350 `.sram8`) as used RAM and left a zero-size stack (fix in knurling-rs/flip-link#142, not
  released as of 19 Oct 2026). Applied to usb_io_fw and typec_dongle.
  The superseded `bedrock_template/` (TPL-20) still has the old line.
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
- 🧪 **CLI-10 Install the tools**: every generated project has `just setup` (`scripts/setup.py`, TPL-36): rustup with
  the firmware's toolchain and target, probe-rs, flip-link, cnt, ww, espup / espflash as the answers need; root steps
  printed. binutils (`cargo-binutils`, `llvm-tools`) not yet.
- 💡 **CLI-11 Terminal to the firmware**, embassy CPU load, ETM.

## Testing support (`TST`)

- 📋 **TST-1 Host unit tests** in generated firmware (`#![cfg_attr(not(test), no_std)]`).
- 📋 **TST-2 On-target tests** with embedded-test (mcan's HIL crates are an example).
- 💡 **TST-3 Property tests** (`quickcheck`) and embedded-hal mocks.

## Docs (`DOC`)

- 🚧 **DOC-1 Book** (`book/`): common issues (defmt section missing, memory range errors); build info and counters
  pages are empty.
- ✅ **DOC-2 Skill docs**: `SKILL.md`, `references/` (frameworks and tested matrix, memory layout, STM32 notes).

## Board map (`MAP`)

Board infographics for the firmware templates and any dev board: `bedrock_board` (library) and `boardmap`
(`bedrock_board_cli`). The facts come from the board's PCB file (a vendor's Altium or Allegro board through
`kicad-cli pcb import`) and a TOML description (what to draw, what every pin does); the pictures are SVG (PNG on
request, resvg). Example: `firmware_template_skill/scripts/data/boards/nucleo_g0b1re/` (NUCLEO-G0B1RE, MB1360 rev C), the board preset (TPL-45).

- ✅ **MAP-1 Geometry from a `.kicad_pcb`**: `boardmap extract` keeps the footprints of interest (connectors,
  jumpers, solder bridges, LEDs, buttons, chips, crystals) with their pads at absolute positions, nets and side,
  and the outline (`Edge.Cuts` lines, arcs and rectangles, also those an enclosure footprint carries, as on rePort
  B200A), as a small JSON the description points to; the vendor's file is not
  needed afterwards · `bedrock_board/src/geometry.rs`.
- ✅ **MAP-2 Top and bottom views**: the outline, connectors pad by pad with a label each (net name, what the pin
  does, Arduino name, pin number) coloured by role (free, shared, taken, function, power rails), parts of
  interest as labelled boxes, solder bridges with their default state and a few words each; the bottom view is
  mirrored like a board turned over, its labels dodge pads and each other · `views.rs`.
- ✅ **MAP-3 Nets across fitted solder bridges**: a `SB_ON` bridge joins the nets the board file keeps apart, so
  a header pin behind one shows the MCU pin it really carries (CN7-29 is PF0 through SB25) · `nets.rs`.
- ✅ **MAP-4 Power tree**: sources, switches and jumpers, regulators, rails and loads on a grid from the
  description, options off by default dashed · `power.rs`.
- ✅ **MAP-5 Pin table**: every MCU pin with its header positions, what else sits on its net, its role and the
  description's note, plus the solder bridge table (Markdown) · `pins.rs`.
- 📋 **MAP-6 Every template ships its board map**: `docs/board/` with the description, the geometry, the rendered
  pictures and the pin table, `just board` to re-render, the README showing the top view (user, 23 Oct 2026:
  "standard for all templates"). Done for the Nucleo-G0B1RE template; the generator (TPL) should lay the folder
  out for every board it knows.
- 📋 **MAP-7 Label collisions on dense headers**: labels of inside-labelled sockets do not dodge solder bridges
  next to them (CN8 on the Nucleo); a general placement pass would.
- 📋 **MAP-8 Highlight a peripheral**: colour the pins of one function (an SPI, the USB pair) on request, for a
  wiring picture in a docs page.
- ✅ **MAP-9 `boardmap --version` names the git SHA**: version, SHA (`-dirty`) and build time from a small
  `build.rs` (AGENTS.md *Versions*), so `tpm install` takes it: `bedrock_board_cli/build.rs`.
- ✅ **MAP-10 MCU pins named from KiCad's pin functions**: on a board whose nets carry functional labels (`LED`,
  `FDCAN_TX`, B129A CANnify), `extract` keeps each pad's `pinfunction` (`PA11/PA9` → `PA11`) and drops the
  root-sheet `/` of KiCad's net names; the pad's pin name joins its net, so the views colour it by the pin's role
  and the pin table lists `PB14` with its label `LED` (the generator's `--pins board` reads the same); the alias
  column only where a description names any; a `+` in front of a rail (`+VIN_CAN`) keeps its class; the bottom
  view's title drops "(mirrored ...)" when a narrow board has no room for it; `extract --pin-names` renames what a
  symbol calls the pins (`GPIO38_47` → `GP38`, rePort B200A) · `geometry.rs`, `nets.rs`, `pins.rs`.
- ✅ **MAP-11 BGA boards: unconnected balls, shorted pins, horizontal headers**: on the B135B (TFBGA176, TPL-59) a
  pad on KiCad's `unconnected-...` net gets no row in the pin table, only a "Not connected on the board" line, and
  is no free pin for the generator's `--pins board`; two MCU pins on one net (PB3 and PG9) get a row each with the
  other in "also on the net"; pin names with a second function (`PH0-OSC_IN`) count as the pin (`PH0`); net labels
  drop the hierarchical sheet path (`/MCU_H7/PE0` → `PE0`); `labels = "outside"` labels a header lying across the
  board above its top row and below its bottom row, turned on their side · `nets.rs`, `pins.rs`, `views.rs`,
  `bedrock_gen.py` `board_pins()`.

- ✅ **MAP-12 Pin names from symbols that number every pin**: some KiCad symbols name pins `PC13_1` and the G0's
  remap pads `PA9/PA11_33` (B153A Donguru): `extract` drops the `_<pad>` suffix when every pad of a footprint has
  it (not when that would leave a connector's `Pin_1`, `Pin_2` all one name), a remap pad whose first name is
  another pad's takes its other GPIO name (pin 33 is PA11), and views and the pin table show a hierarchical
  sheet's net without its path (`/MCU/IO7_EXT` is `IO7_EXT`) · `geometry.rs`, `nets.rs`.

## Dropped and superseded

- **MAP-14 Two MCU pins on one net in the pin table** (B200A's GP26 / GP34, GP30 / GP39): superseded by MAP-11,
  which fixed the same for the B135B; `pins.rs` test `two_rp_pins_on_one_net_get_a_row_each` covers the RP case.
- **BLD-7 Git hook** that replaces path dependencies with git links before pushing: superseded by BLD-8 (git
  dependencies, patched to the sibling checkout for local work).
