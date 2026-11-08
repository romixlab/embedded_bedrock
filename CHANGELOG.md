# Changelog

Changes to the embedded_bedrock Rust crates (`bedrock_build`, `bedrock_build_info`, `bedrock_crash`, `bedrock_led`,
`bedrock_sync`, `bedrock_can`, `bedrock_usb`, `bedrock_rom_boot`, `bedrock_board`, `bedrock_lint`, `bedrock_clocks`, `bedrock_clocks_rt`, `bedrock`, `bedrock_cli`, `fw_registry`). The firmware template has its own changelog and version:
[firmware_template_skill/CHANGELOG.md](firmware_template_skill/CHANGELOG.md).

The format follows [Keep a Changelog](https://keepachangelog.com/), versions follow
[Semantic Versioning](https://semver.org/). Feature IDs refer to [FEATURES.md](FEATURES.md).

## [Unreleased]

### Added

- `bedrock_usb`, a USB full-speed device stack for STM32 fixed at compile time, first part: the shared types (endpoint address and direction, bulk / interrupt, `Single` / `Double` buffering as a value and as zero-sized types, the setup packet, standard request and descriptor codes), `desc` with the device as a `const` value and every descriptor as `const fn` output into `[u8; N]` (device, configuration with IADs, interfaces and alternate settings, endpoints, class-specific blobs, strings and string 0, BOS with the USB 2.0 extension and the WebUSB and MS OS 2.0 platform capabilities, the MS OS 2.0 descriptor set with function subsets), `layout` with the endpoint registers and USB SRAM of the usb_v4 peripheral (buffer table, packet buffers, initial descriptor words) and its checks against RM0444, and `descriptors!`, which names the arrays and the layout and fails the build on a device the descriptors or the peripheral can't serve. Donguru's descriptor set comes out byte for byte as embassy-usb's `Builder` writes it (golden test) (USBD-1).

## [0.2.51] - 2026-11-07

### Changed

- docs: internal references removed

## [0.2.50] - 2026-11-05

### Changed

- `bedrock_clocks` works in exact_units types: every frequency, limit and target is an `Exact<Hertz>`, VDD an `Exact<Volt>`, a target's tolerance a `Tolerance`, a steer range `Ppm<i32>` ends; the crate's own `ratio` module (`Ratio`, `Hz`) is gone (RCC-1). **Breaking** for code that builds a `Config`.
- `bedrock_clocks` knobs are bounded types: `Divm`, `Divn`, `Divpqr` and `Fracn` are exact_units `Fixed` with the RM0468 field range as their `Range` (FRACN in steps of 1/8192, 0 excluded), HSIDIV, D1CPRE / HPRE and the APB prescalers are enums. A value its register can't hold doesn't construct: `divm`, `divn`, `divpqr`, `fracn`, `HsiDiv::from_ratio`, `Hpre::from_ratio`, `Ppre::from_ratio` and `Pll::from_ratios` refuse it with a `KnobError` naming the field, the allowed values and the RM section, the checks the checker ran on plain integers before (RCC-1).
- `bedrock_clocks::h72x::Regs` holds stm32-metapac register types, built with the PAC's setters (the setters aren't `const`, so `clocks.rs` still carries the words); one mux table serves the checker and the encoder (RCC-3).
- A target outside its tolerance reads as exact_units' `OffTarget`: `79.99998046875 MHz is -0.244140625 ppm off 80 MHz, outside ±0.1 ppm` (RCC-1).

## [0.2.49] - 2026-11-04

### Changed

- Quantity conversions follow wire_weaver STD-102 (`ww_si` on the `exact_units` integer kernel): `bedrock_sync` steer points read the local time with `to_in::<u32>(Micro, Truncate, Wrap)`, `bedrock_can` reads bit rate, clock and bus-off recovery time with `to_in::<u32>(.., Truncate, Checked)`. Needs `cargo update -p wire_weaver -p exact_units`; the results on integers are the same.

## [0.2.48] - 2026-11-04

### Changed

- `exact_units` is a git dependency without `rev` (Cargo.lock pins the commit); a commented-out `[patch]` block in the workspace `Cargo.toml` switches to `../exact_units` for local work. The next build needs `cargo update -p exact_units`, and `cargo tree -d` must show one exact_units

## [0.2.47] - 2026-11-03

### Added

- `bedrock_crash::rp::UPDATE_PENDING`, `update_pending` and `ResetFlags::updated`: `Updated` carried through BOOTSEL in the same scratch register, since the boot ROM erases all SRAM (and the record) before BOOTSEL (ROB-14).

### Fixed

- `bedrock_crash::rp`: RP2040 / RP2350 crash records no longer vanish as a power loss (ROB-14). `CHIP_RESET` keeps `HAD_POR` from the power-up across `SYSRESETREQ` and the boot ROM's reboots, so every reset read `PowerOn` and `boot` wiped the record. New `CHIP_RESET_SCRATCH`, `CHIP_RESET_SEEN` and `chip_reset_fresh`: a marker in `WATCHDOG.SCRATCH1`, which every chip-level reset clears, tells a fresh `CHIP_RESET` from a stale one.

## [0.2.46] - 2026-11-03

### Changed

- WireWeaver comes from git only, pinned to a commit in `Cargo.lock`: the committed `[patch]` in
  `.cargo/config.toml` that pointed it at the sibling `../wire_weaver` is gone, so `--locked` lint and test give the
  same result on every PC, whatever commit (if any) the sibling is at. Co-development: `just dev-local` / `just
  dev-git` switch a gitignored `.cargo/dev-local.toml` (made from `.cargo/sibling-patch.toml`, included by
  `.cargo/config.toml`); bump with `cargo update -p wire_weaver` (BLD-10).

## [0.2.45] - 2026-11-03

### Changed

- `just test-all` (the release gate's full run) uses `cargo nextest` instead of four separate `cargo test`
  invocations, cuts test-binary debug info to `line-tables-only` with `split-debuginfo = "unpacked"`
  (`[profile.test]`), and folds the `bedrock_led` / `bedrock_sync` / `bedrock_rom_boot` extra-feature passes into
  one nextest run (`-p bedrock_led -p bedrock_sync -p bedrock_rom_boot --features pkg/feature,...`) instead of
  three separate `-p PKG --all-features` runs, so `wire_weaver` and `embassy-time` compile once for all three
  instead of once per crate; the no-extra-features workspace run stays separate, since it is the only run that
  still exercises `bedrock_led` / `bedrock_sync` / `bedrock_rom_boot`'s HAL-free, feature-off build. Doctests still
  run through plain `cargo test --doc` (nextest doesn't run them) for both feature sets.
  Same 234 nextest-run tests and 3 passed doctests (237 total) before and after, verified by `cargo nextest list`
  and the doctest pass/ignore counts, name for name. Measured on one of our PCs (14 cores; its global cargo config
  already set `debug = "line-tables-only"` for `profile.test`, so that part of the win doesn't show here, only
  `split-debuginfo` and the folded feature passes do): clean target dir, 14 jobs, 111.4s/4.07 GB/5.5G →
  113.1s/3.58 GB/4.8G; clean, 8 jobs, 119.9s/2.65 GB/5.5G → 101.5s/2.52 GB/4.8G; warm target dir is ~2-3s and
  under 0.25 GB either way, before and after. The gate's own learned peak was 19.5 GB at 24 jobs on another of our PCs for
  only 237 tests that each run in under 0.1s, all in compiling and linking (2026-11-03).

## [0.2.44] - 2026-11-03

### Added

- `bedrock_clocks`: exact STM32H72x/73x (RM0468) clock planning on the host. The tree is evaluated as exact
  rationals, checked against RM0468 Rev 3, DS13311 Rev 3, ES0491 Rev 9 and AN5337 Rev 4 with every finding naming
  its config key, value, range and document section, and written as a reviewable `clocks.rs` (register words with
  their fields, exact frequencies as exact_units constants, FRACN steer ranges). Every voltage scale VOS0..VOS3 is
  supported with its own limits and flash wait states; VOS0 needs `allow.vos0_lifetime`, the CPU boost
  `allow.cpu_boost_tcm_ecc_off` (RCC-1, RCC-3).
- `bedrock_clocks_rt`: applies a `clocks.rs` plan at boot on the H72x/73x without a PAC: fixed writes and bounded
  ready-waits in the RM0468 order, the supply configuration written once and read back (a clear error when an
  earlier writer locked another one), `verify` readback naming the first register that differs, and `set_fracn`
  within the planned steering range, holding PLLxFRACEN low long enough for the new FRACN to latch (RCC-4).

## [0.2.43] - 2026-11-03

### Fixed

- `firmware_template_skill` 0.2.43: firmware_template_skill/CHANGELOG.md, section [Unreleased]

## [0.2.42] - 2026-11-03

### Added

- `bedrock-lint` executor checks EXE-1 (deadline task beside bulk work on the thread executor, a candidate), EXE-2
  (blocking call in interrupt context), EXE-4 (interrupt executor not below the interrupts that wake it), EXE-5
  (executor on a vector a driver binds or a used peripheral raises), EXE-6 (state shared across contexts behind a
  thread-only mutex or `static mut`), EXE-7 (unbounded drain or log loop without a yield), EXE-8 (request handler
  awaiting a bus), EXE-9 (no-op waker), EXE-12 (`async fn` that never awaits) and EXE-13 (ring reader without
  batching). The model resolves calls by name and traces each spawner to its executor, so a check knows which code
  runs in interrupt context and which in thread mode (LINT-3).
- `bedrock-lint` interrupt and DMA source checks: IRQ-1 (flags cleared wholesale), IRQ-2 (status register cleared
  with `modify()`, always a candidate), IRQ-4 (several drivers on one shared NVIC vector), IRQ-5 (interrupt
  unmasked without a handler bound or `bind_interrupts!` binding, and M_CAN `ILS` routed without `ILE`), IRQ-6
  (nested disable / enable of interrupts by hand), IRQ-8 (`DefaultHandler` missing, or present but not counting
  or logging), DMA-2 (D-cache on with DMA in use and no clean / invalidate or MPU setup, on an M7 part), DMA-4
  (a DMA ring consumer whose overrun isn't counted with `cnt!`) and DMA-6 (STM32G0B1 / G0C1 UCPD_TXDR written by
  software instead of DMA, ES0548 2.17.1) (LINT-3).

## [0.2.41] - 2026-11-03

### Added

- bedrock-lint source checks for boot, supply, flash, clocks, errata, external chips, USB and logging: BOOT-1,
  BOOT-2, BOOT-10, PWR-1, PWR-3, PWR-4, FLS-1 to FLS-4, CLK-3, CLK-5, ERR-2, ERR-4 to ERR-8, ERR-11, EXT-4, EXT-7,
  EXT-12, USB-1, USB-3, LOG-1, and the source half of SIZ-5 beside its ELF half (`Debug2Format`, a panic handler
  that formats, `unwrap` on a sure `Result`), each with a fixture; chip-limited ones run on the families they name
  (LINT-11).

## [0.2.40] - 2026-11-03

### Added

- `bedrock-lint` checks on host input: HOST-1 (a panic a host value reaches in a WireWeaver handler or a function it
  hands the value to: `unwrap`, indexing, division, overflow, `assert!`, `todo!`), HOST-3 (a host value's unknown
  variant mapped to a default instead of refused), HOST-5 (a truncated or oversized request refused without a
  counter) (LINT-6).
- `bedrock-lint` counter checks: CNT-1 (an `Err` arm, `if let Err`, `is_err()`, `let Ok(..) else` or a select's
  timer arm that neither counts, hands the error on nor panics) and CNT-2 (`&mut`, `as_mut_ptr` or a u64 pointer on
  the `cnt` counter buffers) (LINT-6).
- `bedrock-lint` fault checks: FLT-1 (HardFault missing or spinning), FLT-2 (defmt, `cnt!`, `unwrap` or a panic in
  HardFault / DefaultHandler), FLT-3 (`exc_return!()` not HardFault's first statement), FLT-4 (UsageFault,
  MemManage, BusFault handlers missing or not enabled on M3 and up), FLT-5 (reset cause never read), FLT-7 (no
  watchdog, or one fed from an interrupt or a loop that only waits), FLT-8 (no boot-loop rescue), FLT-9 (USB
  firmware that waits for a healthy mark but never calls `usb_configured()`) (LINT-6).
- `bedrock-lint` pin checks: PIN-1 (a GPIO driver dropped at once or when `main` returns), PIN-2 (an `.await`, a
  spawn or a USB device before the first output is set up), PIN-4 (RP FUNCSEL switched at run time, a GPIO on a
  stolen pin), PIN-6 (a pin taken with `steal()` that has another owner), PIN-8 (unused pins left floating on STM32
  families that reset to inputs), PIN-9 (a pin map chosen by a cargo feature), PIN-16 (G0 pin read before the UCPD
  strobe, or the strobe cleared again) (LINT-6).

## [0.2.39] - 2026-11-03

### Added

- `bedrock_lint` config checks (LINT-4): CFG-1 (probe-rs runner without `--probe VID:PID`), CFG-2 (probe-rs
  `download` + `reset` chained on F0/G0/L0), CFG-4 (a git dependency not covered by an active `[patch]` on its
  source), CFG-5 (embassy-stm32's chip feature not the board's chip, STM32 only), CFG-6 (thumbv6m without
  portable-atomic's critical-section feature where static_cell needs CAS), CFG-7 (no bedrock_build dependency, so
  a dirty build carries no mark), SIZ-3 (a per-package `opt-level` override under `lto = "fat"`, which does
  nothing), SIZ-4 (release profile missing `codegen-units = 1`, `lto` or `panic = "abort"`), LOG-2
  (`DEFMT_LOG`/`DEFMT_RTT_BUFFER_SIZE` not set explicitly when defmt is a dependency), CLK-2 (embassy-stm32's
  `time-driver-any` feature, the cfg half), MEM-1 (`FLASH`'s length in memory.x reaching into a reserved region),
  MEM-2 (a NOLOAD RAM section with more than a `*(...)` line in its body), MEM-7 (a flash partition's helper
  symbols written as an absolute address while a sibling's subtract the flash base, STM32's fixed flash address
  space tells a partition from a RAM helper symbol), MEM-12 (defmt without a `-Tdefmt.x` build-script directive,
  the cfg half), CNT-3 (a `*-macros`/`*-derive` crate and its runtime crate not pinned to the exact same version),
  EXE-10 (embassy-executor's `platform-cortex-m` together with a HAL's own `executor-thread`/`executor-interrupt`).
  CFG-3, CFG-8, SIZ-2, MEM-8 and MEM-11 are not automated yet: each needs more than `Cargo.toml`, the cargo
  configuration, `build.rs`, `memory.x` and `bedrock_fw.json` give (the justfile, CI knowledge, or a per-chip flash
  page-size table), see FEATURES.md LINT-4.

## [0.2.38] - 2026-11-03

### Changed

- `bedrock_crash`: a panic record keeps the return addresses found on the stack (up to 32, innermost first), the call chain to the panic site, instead of the 32 words above `capture_panic`'s own frame (ROB-13). `is_return_address` is the test it uses (Thumb bit, inside `.text`, after a `BL` / `BLX`). A host turns them into `file:line` with the ELF, which keeps panics readable in firmware built without panic locations (`-Zlocation-detail=none`). Fault records are unchanged.

## [0.2.37] - 2026-11-03

### Fixed

- The workspace builds from a fresh checkout again: `.cargo/config.toml`'s `[patch."https://github.com/vhrdtech/wire_weaver"]`
  names `shrink_wrap` too, so exact_units (pulled in by `ww_si`, used by `bedrock_sync`) uses the sibling
  checkout's shrink_wrap and not a second one from git (E0277 `SeriesCells`/`SeriesColumns`/`SeriesStore` in
  `ww_si`). `just test` and `just lint` pass clean again.
- `just lint` and `just test` pass `--locked` on every `cargo` call: the `../wire_weaver` path patch makes
  `Cargo.lock` sensitive to whatever commit that sibling checkout is at, and without `--locked` a moved sibling
  made these gate recipes silently rewrite `Cargo.lock` instead of erroring.

## [0.2.36] - 2026-11-03

### Added

- `bedrock_lint` and its CLI `bedrock-lint` (crate `bedrock_lint_cli`): reads a firmware crate (source with syn,
  `Cargo.toml`, `.cargo/config.toml`, `build.rs`, `memory.x`, `bedrock_fw.json`) into a model and runs the
  automatic checks of the catalogue on it: first STK-1 (flip-link not the linker), STK-2 (unknown `cargo:` key in
  build.rs), STK-4 (cortex-m-rt `paint-stack` on) and EXE-3 (interrupt executor started before its priority is
  set). Text, JSON and SARIF reports, waivers with a reason in the firmware's files, a baseline, `explain`,
  `checks list`, `model` (LINT-2).
- `bedrock-lint check --elf <ELF>` runs the elf checks on the linked firmware: STK-3 (stack not below .data / .bss,
  or no room), STK-5 (a `StaticCell::init` by value with a big frame), STK-6 (a frame over the budget, a worst-case
  stack depth over the stack, recursion), SIZ-1 (a memory.x region over 90 % used), SIZ-5 (core::fmt linked in, and
  from where), SIZ-6 (a big `.data` initializer), MEM-3 (startup init over `.uninit`), MEM-6 (BKP counters not at the
  backup registers), MEM-10 (an RWX segment or a writable section in flash), DMA-1 (H7 DMA buffers in DTCM), PERF-2
  (64-bit and 32-bit division and multiply routines on an M0+, with the callers), PERF-3 (a RAM function inlined
  back into flash) and ERR-3 (G0B1 code in flash bank 2). Thresholds in `[limits]` of `bedrock-lint.toml`. Frames
  and calls are read from the Thumb code (prologues, `bl`), without a disassembler (LINT-5).
- `bedrock-lint frames <ELF>`: the largest stack frames and the deepest call path from `Reset` (LINT-5).

## [0.2.35] - 2026-11-03

### Added

- `bedrock_led::code` (DEV-18): blink codes, the same on every board, for a firmware that won't start: one long flash, then `Code` short flashes, a pause, repeated for about 10 s (`DURATION_MS`), red (`COLOUR`) on an RGB LED. `blink(code, set_led, delay_ms)` is blocking and HAL-free, so it runs before any init; `steps` and `period_ms` give the pattern. Code 1 `UnknownRework` (wire_weaver STD-101: a rework the image doesn't know and no allowance), code 2 `RecordUnreadable`. The table a person reads is `book/blink_codes.md`.

## [0.2.34] - 2026-11-03

### Added

- Detail pages for the catalogue's PIN, CLK, ERR, EXT, USB, HOST, CFG, PERF and TEST categories
  (`book/checks/{pin,clk,err,ext,usb,host,cfg,perf,test}.md`): what goes wrong, why, our example, the fix, how
  it's detected, and the errata quoted from the document where one applies, linked from `book/checks.md` (LINT-8).

## [0.2.33] - 2026-11-03

### Added

- `bedrock_lint` and its CLI `bedrock-lint` (crate `bedrock_lint_cli`): reads a firmware crate (source with syn,
  `Cargo.toml`, `.cargo/config.toml`, `build.rs`, `memory.x`, `bedrock_fw.json`) into a model and runs the
  automatic checks of the catalogue on it: first STK-1 (flip-link not the linker), STK-2 (unknown `cargo:` key in
  build.rs), STK-4 (cortex-m-rt `paint-stack` on) and EXE-3 (interrupt executor started before its priority is
  set). Text, JSON and SARIF reports, waivers with a reason in the firmware's files, a baseline, `explain`,
  `checks list`, `model` (LINT-2).

## [0.2.32] - 2026-11-03

### Added

- Detail pages under `book/checks/` for the first half of the catalogue (EXE, IRQ, DMA, STK, MEM, SIZ, FLT, BOOT,
  PWR, FLS, LOG, CNT): what each check catches, why it matters, an example, how it's checked and the fix (LINT-7).

## [0.2.31] - 2026-11-03

### Added

- Firmware check catalogue `book/checks.md`: 177 checks from our boards' bugs and the firmware checklist, by ID,
  severity and where each is checked, with a review checklist and a bring-up checklist condensed from it (LINT-1).

## [0.2.30] - 2026-10-29

### Added

- `boardmap extract --rename PATTERN=REPLACEMENT` renames a pad's net/pin by a whole-string regex (`$1` groups,
  repeatable, first match wins) before anything else sees it, for a vendor schematic that names a pin its own way:
  Raspberry Pi's own Pico design calls its GPIOs `GPIO0`, `GPIO26_ADC0`, ...; `--rename '^GPIO(\d+)(_ADC\d)?$=GP$1'`
  turns them into the `GP0`.. names the rest of boardmap and the generator expect (MAP-13).

## [0.2.29] - 2026-10-29

### Added

- `boardmap extract --pin-names REGEX=REPLACEMENT` renames the pin names a symbol gives the MCU's pads
  (`GPIO38_47` → `GP38` for rePort B200A's RP2350B) (MAP-10).

### Fixed

- boardmap extract reads the outline an enclosure footprint carries (`fp_line` / `fp_arc` on `Edge.Cuts`, KiCad 10
  boards like rePort B200A) and `gr_rect` outlines; before, such a board failed with "no board outline" (MAP-1).

## [0.2.28] - 2026-10-29

### Added

- boardmap: `labels = "outside"` for a header lying across the board, its labels turned on their side above the top
  row and below the bottom row (MAP-11).

### Fixed

- boardmap on BGA boards: pads on KiCad's `unconnected-...` nets get no pin table row (a "Not connected on the board"
  line lists them), two MCU pins on one net get a row each, `PH0-OSC_IN`-style pin names count as the pin, and net
  labels drop the hierarchical sheet path (MAP-11).

## [0.2.27] - 2026-10-29

### Fixed

- `firmware_template_skill` 0.2.27: firmware_template_skill/CHANGELOG.md, section [Unreleased]

## [0.2.26] - 2026-10-29

### Fixed

- boardmap reads MCU pin names from symbols that append the pad number (`PC13_1`) and name the G0's remap pads
  either way round (`PA9/PA11`), and shows hierarchical sheets' nets without their path (MAP-12).

## [0.2.25] - 2026-10-29

### Fixed

- `firmware_template_skill` 0.2.25: firmware_template_skill/CHANGELOG.md, section [Unreleased]

## [0.2.24] - 2026-10-29

### Added

- boardmap names MCU pins after the pads' KiCad pin functions where the nets carry functional labels (`LED` is
  `PB14`), and drops KiCad's root-sheet `/` from net names; the pin table shows the label next to the pin (MAP-10).

### Changed

- boardmap: the pin table leaves out the Arduino column on a board without aliases, "taken" also covers a pin wired
  to a part of the board for good, `+5V0`-style rails get their power colour, and a narrow board's bottom view
  shortens its title (MAP-10).

## [0.2.23] - 2026-10-29

### Fixed

- `firmware_template_skill` 0.2.23: firmware_template_skill/CHANGELOG.md, section [Unreleased]

## [0.2.22] - 2026-10-29

### Added

- `bedrock_can` 0.1.0: an FDCAN (M_CAN) channel as `ww_can_bus::Can` without a HAL, from B135's `b135_board::can`
  with the kernel clock and message RAM layout as parameters (`Controller::h7`, `h7_half`, `g0`): bit timing,
  configuration and filter checks, frames to send, error state (DEV-17)

## [0.2.21] - 2026-10-28

### Fixed

- `firmware_template_skill` 0.2.21: firmware_template_skill/CHANGELOG.md, section [Unreleased]

## [0.2.20] - 2026-10-28

### Fixed

- `firmware_template_skill` 0.2.20: firmware_template_skill/CHANGELOG.md, section [Unreleased]

## [0.2.19] - 2026-10-28

### Fixed

- `firmware_template_skill` 0.2.19: firmware_template_skill/CHANGELOG.md, section [Unreleased]

## [0.2.18] - 2026-10-25

### Added

- `boardmap --version` names the git SHA and build time, so an installed build can be identified exactly (MAP-9).

### Changed

- `bedrock_board`'s NUCLEO-G0B1RE example moved to the template's board preset
  (`firmware_template_skill/scripts/data/boards/nucleo_g0b1re/`), one copy for both (MAP, TPL-45).

## [0.2.17] - 2026-10-24

### Fixed

- `firmware_template_skill` 0.2.17: firmware_template_skill/CHANGELOG.md, section [Unreleased]

## [0.2.16] - 2026-10-24

### Added

- `bedrock_build::serialize_compact_build_info`: `compact()` alone, for a crate without defmt (BLD-2).

### Changed

- The workspace takes WireWeaver by git, not by path; local work builds against `../wire_weaver` through the
  `[patch]` in `.cargo/config.toml` (BLD-8). `bedrock_build` / `bedrock_build_info` taken from git, and
  `cargo install --git https://github.com/romixlab/embedded_bedrock bedrock_board_cli`, now build on a PC without a
  wire_weaver checkout. `bedrock_led`, `bedrock_sync` and `bedrock_rom_boot` take their WireWeaver crates through
  the workspace too (`bedrock_rom_boot` with `ww-dfu` resolves from git).

### Fixed

- `bedrock_build` on `build-info-common` 0.0.46, the version `build-info-build` in the template uses: firmware
  build scripts failed with mismatched `BuildInfo` types (BLD-2).

## [0.2.15] - 2026-10-23

### Added

- `bedrock_rom_boot` (DEV-16): firmware updates through the STM32 ROM bootloader for G0 and H7 (H72x-75x), taken from Donguru and B135: boot word in the last backup register (Donguru's format, so Donguru keeps reading it), `early()` before the HAL's init jumps into the ROM on request (AN2606) and resets once after the ROM's leave, boot-loop rescue after three boots without a healthy mark, the ROM's USB serial number; feature `embassy` for the reboot task and the healthy watch, feature `ww-dfu` for `ww_dfu` `BootControl` and `UpdateNode` answers (`Profile::InPlace`, always `Confirmed`, `reboot(Updater)` into the ROM). No HAL dependency: raw RM addresses.

## [0.2.14] - 2026-10-23

### Fixed

- `firmware_template_skill` 0.2.14: firmware_template_skill/CHANGELOG.md, section [Unreleased]

## [0.2.13] - 2026-10-23

### Added

- `bedrock_board` and `boardmap` (MAP-1..5): board infographics from a PCB file and a TOML description: top and
  bottom views with every header pin labelled and coloured by what it is for, solder bridges with their default,
  a power tree and a pin table; nets joined across fitted solder bridges; SVG and PNG. Example and first user: the
  NUCLEO-G0B1RE (`bedrock_board/examples/nucleo_g0b1re/`).

## [0.2.12] - 2026-10-22

### Added

- `just test --changed` runs only the workspace crates changed against main and their dependents (nextest's `rdeps()`); plain `just test` runs everything, as `just test-all` does.

## [0.2.11] - 2026-10-22

### Fixed

- `firmware_template_skill` 0.2.11: firmware_template_skill/CHANGELOG.md, section [Unreleased]

## [0.2.10] - 2026-10-22

### Added

- `bedrock_sync` (DEV-13, wire_weaver SRV-31): one steerer at a time. `Steer::steerer` (the point's
  `ww_timebase::Steerer::id`, 0 for points that name nobody); of the host's points, `SharedClock` follows one steerer
  while its points keep coming within `Config::holdover_after_us` (3 s) and only notes another one's
  (`Steered::OtherSteerer`); once the followed one has been quiet that long, the next steerer's point takes over,
  slewed and with a new `generation` as any switch. `SharedClock::steerer`, `SharedClock::config`.
  `ClockServer::sync_status` fills `SyncStatus::steerer` (the followed or last followed host) and `noted` (the last
  other steerer, within the holdover time), names kept in fixed buffers of `clock::NAME_MAX` (24) bytes, cut at a
  char boundary.

### Changed

- `bedrock_sync`: `ClockServer::steer` / `steer_with` take `&mut self`; `Steer` has the new `steerer` field (0 for
  a sync line's points), `Steered` the new `OtherSteerer` variant; `sync_status` returns `SyncStatus<'_>` (the
  wire_weaver `ww_timebase` types carry a lifetime now).

## [0.2.9] - 2026-10-21

### Added

- `bedrock_led::look` (DEV-15): the bedrock look, what a status LED shows for what it means, the same on every
  board: `OK` (3 s breath), `BUSY` and `WARN` (1.5 s breath), `FAULT` (steady), `ALERT` (250 ms blink),
  `HEARTBEAT` (300 ms flash every 1.5 s), `IDENTIFY`, `OFF`, as `Look { mode, colour }` with the palette in
  `look::colour` (`OK` green, `WARN` amber, `FAULT` red, `STORAGE` blue), `Look::in_colour`, `Look::under` (the
  `ww_indication` mode, feature `ww-indication`). Every period divides `CYCLE_MS` (3 s), so on shared time all looks
  of all boards start together.
- `bedrock_sync` (DEV-13): `State::Acquiring`, `Config::lock_within_us` (2 ms) and `Config::acquire_slew_ppm` (5 %),
  `SharedClock::slew_left_at`; `ClockServer::sync_status` fills `SyncStatus::slew_us`.

### Changed

- `bedrock_sync` (DEV-13): `Locked` means within the tolerance now, not just "points come in": the clock is
  `Acquiring` while the offset still to slew plus the source's error is above `lock_within_us`, `Locked` once within,
  and back to `Acquiring` when a point finds it more than twice that off or steps it. While acquiring it slews at 5 %
  (250 ms in 5 s; was 1 %, 25 s), once locked at 1 % as before; shared time still never jumps or goes back.

## [0.2.8] - 2026-10-21

### Added

- `bedrock_sync` (DEV-13): `ClockServer::steer_with`, `Clock::steer` on a board with an `Oscillator` backend (first user: B135's H725 PLL1 / PLL2 FRACN).

## [0.2.7] - 2026-10-21

### Added

- `bedrock_sync` 0.1.0 (DEV-13): the shared clock for bedrock firmware, the device side of wire_weaver's host clock
  sync. `SharedClock` disciplines the local clock onto UTC from steer points (a locked sync line beats the host),
  slews instead of stepping, holdover and free with a growing error estimate, an explicit source list (`Sources`:
  `[Line(IrigB)]` never lets a PC steer the board), `local_at` for scheduled actions; no
  libcall on the Cortex-M0+ in the per-tick path. `Oscillator` lets a chip with a fractional PLL take the rate in
  hardware. Feature `embassy`: `Sync` in a `static` with `wait_until_shared`; `ww-timebase`: `ClockServer`, the
  bookkeeping of a `ww_timebase::Clock` server.
- `bedrock_led` (DEV-14): animations aligned to shared time, `level = f(t mod period)` on UTC, so boards breathe in
  phase: `SharedTime`, `Animation::align`, `Leds::align`, `Indication::run_with` (shared time read every tick).
  Without shared time everything runs as before; per-tick math stays 32-bit on the Cortex-M0+.

## [0.2.6] - 2026-10-21

### Fixed

- `bedrock_template/`: the manifests are `Cargo.toml.liquid` (cargo-generate drops the suffix), so cargo no longer
  prints parse errors for them when a firmware takes `bedrock_crash` from git (TPL-20).

## [0.2.5] - 2026-10-21

### Added

- `bedrock_led` 0.1.0: status LED animation for bedrock firmware (DEV-12), taken out of Donguru and rePort. Off / On /
  Breath / Blink with eased fades, gamma, a perceptual peak brightness and an identify blink, u32 math only (the
  smoothstep exact and monotonic without 64-bit libcalls), HAL-free and host-tested; `Leds<N>` for several LEDs and
  RGB LEDs (colour as per-channel peak) under one indication; `Pwm` over embedded-hal `SetDutyCycle` with an
  active-low option; feature `embassy` for the `static` indication (in `.bss`, one critical section per tick) and its
  task loop, `ww-indication` for the brightness peaks and identify bookkeeping of a `ww_indication::Indication`
  server.
- `just test` and `just lint` for the workspace.

## [0.2.4] - 2026-10-19

### Fixed

- `firmware_template_skill` 0.2.4: firmware_template_skill/CHANGELOG.md, section [Unreleased]

## [0.2.3] - 2026-10-19

### Fixed

- `firmware_template_skill` 0.2.3: firmware_template_skill/CHANGELOG.md, section [Unreleased]

## [0.2.2] - 2026-10-16

### Changed

- Docs: product names written WireWeaver and ShrinkWrap in prose

## [0.2.1] - 2026-10-16

### Added

- `bedrock_crash` 0.1.0: crash capture for Cortex-M firmware (ROB-11). A `HardFault` / panic handler saves cause,
  uptime, registers and fault status, up to 32 stack words, a message and the build id into a 376-byte `.uninit`
  record with magic and CRC-32, then resets; `boot()` keeps it across resets (never reads RAM after power loss),
  counts boots (and the boots since the crash was saved, `Crash::boots_since`, 0 = the last reset was the crash) and
  gives a `Snapshot` for a `ww_firmware_info::LastReset` server; `clear()`, `reset_with(Updated)`.
  Reset-cause decoding for STM32 (`RCC_CSR` / `RCC_RSR` flags) and RP2040 / RP235x (`CHIP_RESET`,
  `WATCHDOG.REASON`). No WireWeaver dependency.

## [0.2.0] - 2026-10-07

### Added

- AGENTS.md (repository rules), FEATURES.md (replaces the README checklists, includes the device contract) and this
  changelog.

## [0.1.0] - 2026-09-29

Not published. Reconstructed from git history.

### Added

- `bedrock_build_info`: build-info types with CRC, serialized with WireWeaver (BLD-1).
- `bedrock_build`: build-script helper embedding build info, RAM linker script (BLD-2, BLD-6).
- `bedrock`: probe-rs prototype reading build info from a target, ELF symbol scan (CLI-2).
- `bedrock_cli`, `fw_registry`: empty crates (CLI-1).
- `bedrock_template`: cargo-generate firmware template (TPL-20), superseded by `firmware_template_skill/` from
  September 2026.
