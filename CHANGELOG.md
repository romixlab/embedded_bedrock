# Changelog

Changes to the embedded_bedrock Rust crates (`bedrock_build`, `bedrock_build_info`, `bedrock`, `bedrock_cli`,
`fw_registry`). The firmware template has its own changelog and version:
[firmware_template_skill/CHANGELOG.md](firmware_template_skill/CHANGELOG.md).

The format follows [Keep a Changelog](https://keepachangelog.com/), versions follow
[Semantic Versioning](https://semver.org/). Feature IDs refer to [FEATURES.md](FEATURES.md).

## [Unreleased]

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
