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
- 📋 **TPL-18 Board pin maps**: generate peripheral and pin assignments from a board description instead of only the
  LED pin.
- 📋 **TPL-19 Ozone and GDB / LLDB start scripts** in generated projects.
- 🔍 **TPL-20 Old cargo-generate template** (`bedrock_template/`): superseded by the skill; check nothing still uses
  it, then move it to *Dropped*. Its manifests are `Cargo.toml.liquid`: cargo scans every `Cargo.toml` of a git
  dependency's repo, and the template tags in them gave parse errors in firmware taking `bedrock_crash` from git.
- 📋 **TPL-21 Hardware test matrix**: bootloader update cycle, settings stored across a power cut, BKP counters across reset, WireWeaver USB
  enumeration, on at least one STM32, RP and nRF board.
- 📋 **TPL-24 Settings store with `--bootloader`**: the embassy-boot updater in `main` and `src/settings.rs` both
  take the `FLASH` peripheral; share one `Flash` (blocking mutex, as the updater already uses) and generate the
  settings code for bootloader layouts too. Until then `--settings --bootloader` reserves the region only.
- 📋 **TPL-25 Settings store on RP and nRF** (and the HAL frameworks): `ww_settings` on embassy-rp's flash (XIP
  stalls during an erase, run from RAM or accept it) and embassy-nrf's NVMC; region only so far.
- 📋 **TPL-26 Settings store on other ECC flash** (G4, L4, L5, U5, WB, WL, H5, C0, U0): needs an `EccSafeFlash`
  family in WireWeaver's `ww_ecc_flash` first (`FLASH_ECCR` address and NMI flow per family); region only so far.
- 🐛 **TPL-23 `init_ram.rs` not rustfmt clean for long bank names**: `xf3` (F303VC), `xg4` (G474RE) and `xl4`
  (L476RG) of the tested matrix fail `rustfmt --edition 2024 --check`: the `let count = ...` line for banks like
  `CCMRAM_ICODE` is over 100 columns and rustfmt breaks it after `=`. Found 15 Oct 2026.
- ✅ **TPL-27 `just flash-bootsel` for RP targets**: a generated `justfile` (`app/justfile.j2`, `t.family == "rp"`
  only, both the plain and `--wire-weaver` layouts) with one recipe, `flash-bootsel [SERIAL]`: build release, then
  `ww bootsel flash` (wire_weaver CLT-21) writes the ELF straight through PICOBOOT in the RP2040/RP2350 boot ROM
  (BOOTSEL mode), no probe, picotool, elf2uf2 or objcopy step; `SERIAL` waits for the device to (re)appear and
  come back. `references/frameworks.md`'s RP2040/RP235x alternative-runner line and both app/ww `README.md.j2`
  mention it. probe-rs stays the default `cargo run`/`cargo build` runner in `.cargo/config.toml`, untouched.
  Verified by generating an rp2040, an rp2350b `--wire-weaver` and an STM32 project (no `justfile` for the latter).

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
  that matrix entry uses the 128 KiB F103CB. Options: a smaller default `MAX_MESSAGE_LEN`, leaving introspection out
  on 64 KiB parts, or the tested part moves to F103CB.
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
  and the last noted steerer (`SyncStatus::steerer` / `noted`, names cut to 24 B).
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
