# Changelog

Changes to the embedded_bedrock Rust crates (`bedrock_build`, `bedrock_build_info`, `bedrock_crash`, `bedrock_led`,
`bedrock_sync`, `bedrock_board`, `bedrock`, `bedrock_cli`, `fw_registry`). The firmware template has its own changelog and version:
[firmware_template_skill/CHANGELOG.md](firmware_template_skill/CHANGELOG.md).

The format follows [Keep a Changelog](https://keepachangelog.com/), versions follow
[Semantic Versioning](https://semver.org/). Feature IDs refer to [FEATURES.md](FEATURES.md).

## [Unreleased]

## [0.2.13] - 2026-10-23

### Added

- `bedrock_board` and `boardmap` (MAP-1..5): board infographics from a PCB file and a TOML description: top and
  bottom views with every header pin labelled and coloured by what it is for, solder bridges with their default,
  a power tree and a pin table; nets joined across fitted solder bridges; SVG and PNG. Example and first user: the
  NUCLEO-G0B1RE (`bedrock_board/examples/nucleo_g0b1re/`).

## [0.2.12] - 2026-10-22

### Added

- `just test --changed` runs only the workspace crates changed against main and their dependents (`tpm test changed`, nextest's `rdeps()`); plain `just test` runs everything, as `just test-all` does (tpm ops#22oct-2049).

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
