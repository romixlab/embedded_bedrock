# Changelog

Changes to the embedded_bedrock Rust crates (`bedrock_build`, `bedrock_build_info`, `bedrock_crash`, `bedrock`,
`bedrock_cli`, `fw_registry`). The firmware template has its own changelog and version:
[firmware_template_skill/CHANGELOG.md](firmware_template_skill/CHANGELOG.md).

The format follows [Keep a Changelog](https://keepachangelog.com/), versions follow
[Semantic Versioning](https://semver.org/). Feature IDs refer to [FEATURES.md](FEATURES.md).

## [Unreleased]

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
