# Changelog

Changes to the firmware template (generator, templates, data) that matter for generated firmware. Newest first.
Generated projects record the version and git commit they came from in `bedrock_fw.json`; an upgrading agent
reads the entries newer than that version (see the generated `AGENTS.md`).

Each entry: what changed, and **Upgrade notes** — how an existing firmware picks it up, which answers/options are
involved, what needs a hardware test. Add an entry (bump the version) for every change that alters generated output
or options; purely internal refactors only need a line under the next version.

## [Unreleased]

- `just flash-bootsel [SERIAL]` for RP targets (TPL-27): a generated `justfile` with one recipe — build release,
  then `ww bootsel flash` (wire_weaver CLT-21) writes the ELF straight through PICOBOOT in the RP2040/RP2350 boot
  ROM (BOOTSEL mode), no probe, picotool, elf2uf2 or objcopy step; `SERIAL` waits for the device to (re)appear and
  come back. Lands at the project root for a plain firmware, inside `firmware/` with `--wire-weaver`. probe-rs
  stays the default `cargo run`/`cargo build` runner. `references/frameworks.md`'s RP2040/RP235x alternative-runner
  line and both generated `README.md`s mention it.

  Upgrade notes: regenerating (or a template upgrade) adds `justfile` for existing RP firmwares; nothing else
  changes, no answer involved.

- Docs: product names written WireWeaver and ShrinkWrap in prose.

- Settings store (TPL-5): `--settings` replaces `--config-page`. The `SETTINGS` region (was `CONFIG`, one sector)
  is `--settings-pages` erase pages at the end of flash, which is the end of bank 2 on dual-bank parts so an erase
  doesn't stall code in bank 1; the default is enough pages for 8 KiB and at least 2 (`ww_settings` compacts into the
  next page before erasing one): 4 × 2 KiB on STM32G0B1, 2 × 128 KiB on F4 / H7 (with a note on the alternatives).
  For `--framework embassy` on STM32 G0 / H7 and parts without flash ECC (F0-F4, F7), without `--bootloader`, it also
  generates `src/settings.rs`: `DeviceSettings` (`#[derive(ww_settings::Settings)]`, one field `label`) and
  `SettingsStore` (`load` at boot, `set` + `persist_if_due` for writes once a change settles, `persist` for a write
  before answering, erase ahead after each write) on embassy-stm32's blocking `Flash`, read through
  `ww_ecc_flash::EccSafeFlash` on G0 / H7 so a word whose write a power cut interrupted reads as zeros instead of
  faulting every boot (G0: a `NonMaskableInt` handler in `main.rs` calls `ww_ecc_flash::on_nmi()` first), through
  embassy-embedded-hal's `BlockingAsync` elsewhere. `main` loads it after `embassy_stm32::init` and logs the label;
  without `--wire-weaver` the main loop calls `persist_if_due()`. With `--wire-weaver` the API root gets
  `ww_impl!(label: ww_user_info::UserInfo)` (name kept in flash, written before the call answers; `notes` omitted),
  `ServerState` gets `settings`, and the server task selects between the link and the settings' due time instead of
  `server.run()`. Other targets (RP, nRF, HAL frameworks, other ECC families, `--bootloader`) get the region only,
  with a warning naming TPL-24 / TPL-25 / TPL-26.

Upgrade notes: answer `config_page` is now `settings` (read as such from an old `bedrock_fw.json`, `check-answers`
prints `RENAMED`), new answer `settings_pages` (default 0 = automatic). The region moves and grows: `CONFIG` (the last
sector) becomes `SETTINGS` (the last N sectors), so `FLASH` shrinks (STM32G0B1 512K: `CONFIG` 2K at `0x0807_F800`,
`FLASH` 510K → `SETTINGS` 8K at `0x0807_E000`, `FLASH` 504K); check that the image still fits, and on dual-bank parts
that the code stays in bank 1 (below `BANK_2`) if erases must not stall it. Anything the firmware kept in `CONFIG`
by hand is not migrated: read it out before flashing, or keep its own region. In `memory.x` rename the region and
the `__config_start` / `__config_end` symbols to `SETTINGS` / `__settings_*`. Copy `src/settings.rs`, add `mod
settings;`, the `ww_settings` and `ww_ecc_flash` (G0 / H7, feature `stm32g0` / `stm32h7`) or `embassy-embedded-hal`
dependencies, the NMI handler on G0, and `SettingsStore::load(p.FLASH)` in `main`; a firmware that already owns
`FLASH` elsewhere passes it through. Needs WireWeaver with STD-32 (`ww_settings`) and SRV-19 (`mcu/ww_ecc_flash`,
2ed478f). With `--wire-weaver`: `ww_user_info` (and `ww_error`, `embassy-futures`) in the firmware, `ww_user_info` in
the API crate with its `std` / `defmt` features, the `label` line in the API trait (an API change: bump the API
crate's version), `LABEL_CAPABILITIES` in the `Consts` impl, the two `label_name` handlers, `"label.notes"` in
`omit`, and the server task's `select` loop (or call `persist_if_due()` from the firmware's own loop). A firmware
with its own settings API (Donguru `device.settings`, API-19) keeps its own fields in `DeviceSettings` at new slots.
`--ww-transport rtt` firmware (with or without settings) needs WireWeaver with TR-13: since TR-12 the RTT server
panicked on its first request. Test on hardware: set the label (`ww` over USB), cut the power, check it after the boot; and cut the power while a
write runs (a loop of label writes), the device still boots and has the old or the new label.

- `LastReset` reports the age of the crash (STD-27): `src/last_reset.rs` fills `ResetRecord::boots_since_crash` from
  `bedrock_crash` (`Crash::boots_since`, 0 = the last reset was the crash). **Upgrade notes**: needs the WireWeaver
  with STD-27 and `bedrock_crash` with record layout 2 (a record in RAM from the old layout is dropped, never
  misread); add `boots_since_crash: crash.map_or(0, |c| c.boots_since)` to the `ResetRecord` in `record()`.
- flip-link is really used (ROB-12): `linker = "flip-link"` in `.cargo/config.toml` for Cortex-M targets (application
  and bootloader) instead of `cargo:rustc-linker=flip-link` in `build.rs`, which is no build-script directive: Cargo
  ignored it and every generated firmware linked with plain rust-lld, stack above `.data` / `.bss`. The NOLOAD
  sections for extra RAM banks in `memory.x` no longer end with `. = ALIGN(n)`, so lld drops an empty one: flip-link
  0.1.12 counted the empty `.sram8` at the end of RP2350 RAM as used RAM and left a zero-size stack.

Upgrade notes: answer `no_flip_link` is unchanged. In `.cargo/config.toml` add `linker = "flip-link"` under
`[target.<triple>]` (also in `bootloader/.cargo/config.toml`), delete the `cargo:rustc-linker` line and its comment
from `build.rs`, and in `memory.x` drop `. = ALIGN(n);` from the extra-RAM sections (RP2350 `.sram8` / `.sram9`, STM32
banks). Changes the RAM layout of every firmware: check `_stack_start` equals the start of `.data` and `_stack_end`
the start of `RAM` (`readelf -s` / `-S`). Test on hardware: the firmware boots; a deliberate stack overflow (unbounded
recursion) ends in a HardFault (or a core lockup when the fault frame can't be stacked) instead of corrupting
`.data` / `.bss`. With crash capture the record may be missing after such an overflow when the core locked up.

- Crash capture, on by default on Cortex-M (ROB-11, PLAT-7 for every firmware; `--no-crash-capture` leaves it out).
  New dependency `bedrock_crash` (from `--bedrock`, like `--build-info`). The `HardFault` handler saves registers,
  fault status, 32 stack words and the uptime into a record in `.uninit` RAM and resets instead of looping; a new
  `#[panic_handler]` in `src/last_reset.rs` does the same for panics (replaces panic-probe / panic-rtt-target /
  panic-halt; with a debugger attached it still stops like panic-probe). `last_reset::boot()` in `main` reads the
  reset cause (STM32 `RCC_CSR` / `RCC_RSR` from stm32-data, any framework; RP `CHIP_RESET` + `WATCHDOG.REASON`;
  nRF / ESP not yet), counts boots and logs the last crash.
- GNU build id in every Cortex-M application: `--build-id=sha1` in `build.rs`, `.note.gnu.build-id` kept in flash by
  a `SECTIONS` block at the end of `memory.x` (symbols `__build_id_start` / `__build_id_end`).
- `--wire-weaver`: the API root gets `ww_impl!(firmware: ww_firmware_info::FirmwareInfo)` and, with crash capture,
  `ww_impl!(last_reset: ww_firmware_info::LastReset)`. `ww.rs` serves `FirmwareInfo` from `<Root>Consts` (API-36:
  `FIRMWARE_FIRMWARE = firmware_summary!()`, `firmware_build_id()` from flash, `BUILD_RECORD` omitted) and `LastReset`
  from the boot snapshot; `build.rs` calls `ww_firmware_info::build::emit()`. `ServerState` gets `last_reset`.

Upgrade notes: new option `crash_capture` (answer key `no_crash_capture`, default false = on). For an existing
Cortex-M firmware: add the `memory.x` build-id block and the `--build-id=sha1` line in `build.rs` (check with
`readelf -n`); add `bedrock_crash` to `Cargo.toml`, copy `src/last_reset.rs`, call `last_reset::boot()` right after
logging is set up, replace the HardFault body and drop the panic crate. Firmware with its own RAM init (usb_io_fw's
`init_ram.rs`) must leave cortex-m-rt's `.uninit` untouched, or give the record a region of its own. With
`--wire-weaver`: the API crate gets `ww_firmware_info`, two `ww_impl!` lines (an API change: bump the API crate's
version), the `Consts` impl and three `LastReset` handlers in `ww.rs`, `const_model` and `omit` in `ww_codegen!`.
Test on hardware: force a HardFault (e.g. a read from `0xFFFF_FFF0`) and a panic, check `last reset: Fault` /
`Panic` in the log after the reset and `LastReset` over WireWeaver, symbolize `pc` with the ELF.

- `--wire-weaver`: `src/ww.rs` builds against current WireWeaver master again (TPL-14): handlers take
  `_cx: &mut Context<'_, impl EventOut>`, `process_bytes` takes an `EventWriter` and the transport `Medium` (`()`).

Upgrade notes: firmware generated with 0.5.0 that hasn't adapted yet: change each handler's
`_msg_tx: &mut impl MessageSink` to `_cx: &mut Context<'_, impl EventOut>`, and `process_bytes` as in the new
`ww.rs` (`type Medium = ();`, `out: &mut EventWriter<'_, impl MessageSink>, medium: ()`, passed on to
`process_request_bytes(data, scratch, out, medium)`). Firmware that already adapted by hand (usb_io_fw,
typec_dongle) has nothing to do.

## [0.5.0] - 2026-09-30

- `--wire-weaver`: WireWeaver device API. The output directory becomes a project root with a `#![no_std]` API crate
  (`--ww-api`, default `<name>_api`, blinky `led_on`/`led_off` trait) and the firmware in `firmware/`; `bedrock_fw.json`,
  `AGENTS.md`, `CLAUDE.md` and a project `README.md` stay at the root. `firmware/src/ww.rs` implements the API on
  `ServerState` (owns the LED) and runs the transport. Embassy on STM32 / RP / nRF only.
- `--ww-transport usb` (default, needs `--log defmt`): embassy-usb driver + `wire_weaver_usb_embassy`. STM32 USB
  peripheral, pins and interrupt come from stm32-data; HSI48 + CRS is set up as the 48 MHz USB clock when the part
  has both, otherwise a TODO. RP2040/RP235x and nRF52833/52840 (`usb = true` in `chips.toml`).
- `--ww-transport rtt`: `ww_device::rtt` on `rtt_init!` channels `ww_up`/`ww_down`; log output stays on up channel 0
  (defmt through `rtt-target` instead of `defmt-rtt`, or rprintln's terminal), works with `--log none` too.
- `--ww-src git|<path>`: WireWeaver crates from git (default) or a local checkout. `--bedrock <path>` is now relative
  to the output directory (the same as before without `--wire-weaver`).

Upgrade notes: new options `wire_weaver` (false), `ww_transport`, `ww_api`, `ww_src`; nothing changes for firmware
that does not use them. Adding WireWeaver to an existing firmware is a restructuring (firmware moves into `firmware/`),
do it only on request: generate a fresh project with the recorded answers + `--wire-weaver` and move the firmware's
code over, or keep the layout and copy `src/ww.rs`, the `Cargo.toml` WireWeaver section and the API crate by hand.
Test USB enumeration / RTT connection on hardware.

## [0.4.3] - 2026-09-30

- Generated `.rs` files pass `cargo fmt --check`: `use` items are sorted like rustfmt does (`sort_uses()` in the
  generator), the RP `#[embassy_executor::main(...)]` attribute and flash init are wrapped, the stm32-hal2 nested `use`
  is split into two lines.
- RP235x `memory.x`: `_stext` is 8-aligned (`ALIGN(..., 8)`), fixes the lld warning
  `address of section .text is not a multiple of alignment (8)`.
- `init_ram.rs`: the doc comment no longer says "generated from stm32-data" on RP/nRF; RP says the banks need no enable.
- `build.rs`: no unused `out` / `let _ = out;` on ESP without build info.

Upgrade notes: formatting only for `main.rs`/`build.rs` (running `cargo fmt` gives the same result). RP235x: change
`_stext` in `memory.x` to `ALIGN(ADDR(.start_block) + SIZEOF(.start_block), 8)`, check with `readelf -SW` that
`.text` follows `.start_block`, and boot once on hardware. `init_ram.rs`: comment only.

## [0.4.2] - 2026-09-30

- `build.rs`: `use` lines in rustfmt order, so `cargo fmt --check` passes on it.

Upgrade notes: if `build.rs` is untouched, replace it; otherwise swap `use std::path::PathBuf;` above `use std::{env, fs};`
(or just run `cargo fmt`). No functional change.

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
