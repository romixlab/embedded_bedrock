# Working on embedded_bedrock

Guidance for AI agents and contributors. Read this before changing code.

embedded_bedrock is to firmware what vhrd_egui is to GUIs: the base every in-house bare-metal Rust firmware starts
from. It has two parts:

- **The firmware template** (`firmware_template_skill/`): a Python generator (`scripts/bedrock_gen.py`, Jinja2
  templates, chip data) packaged as the `firmware-template` agent skill. It generates new firmware for STM32, RP,
  nRF and ESP chips (embassy, HAL crates or bare), with memory layout, bootloader, counters, build info and a
  WireWeaver device API, and upgrades existing firmware that records its template version in `bedrock_fw.json`.
  This is the part in daily use (usb_io_fw, typec_dongle).
- **Rust crates** for build info and host tooling: `bedrock_build` (build-script helper that embeds build info),
  `bedrock_build_info` (the build-info format), `bedrock` (host-side probe tool, prototype), and stubs for a CLI and
  a firmware registry.

The device contract every board should meet (bootloader, fw_info / hw_info traits, web app, USB / Ethernet access)
is in FEATURES.md, area `DEV`.

## FEATURES.md is the source of truth

[FEATURES.md](FEATURES.md) lists every feature with its status, every known bug, and what is planned, with stable
IDs per area (`TPL-4`, `DEV-2`, ...). It replaces the README checklists.

- **Read the relevant area before starting.** Template items say which chips and frameworks they cover.
- **Name IDs with a short slug when talking to the user** (answers, plans, summaries, tables):
  `DEV-2 fw-info-trait`, never a bare `DEV-2`. The slug is 2-4 kebab-case words from the item's title. Commit
  messages, CHANGELOG and code `TODO`s keep the bare ID.
- **Update it in the same commit** as the code: mark items ✅/🚧/🧪/🐛 with a pointer to the code, add bugs you
  find but don't fix (next free ID of the area), move obsolete items to *Dropped and superseded*. Never renumber
  or reuse IDs. A template feature is ✅ when generated projects build for the tested matrix; 🧪 until it ran on
  hardware where that matters.
- Don't track status anywhere else (README checklists, TODO files). Code `TODO`s that matter reference an ID:
  `// TODO(BLD-3): ...`.

## Two changelogs

- [firmware_template_skill/CHANGELOG.md](firmware_template_skill/CHANGELOG.md) is the template's changelog, with
  its own version. **Every change that alters generated output or options gets an entry there and a version bump**,
  with *Upgrade notes* for existing firmware: upgrading agents read the entries newer than the version recorded in
  a firmware's `bedrock_fw.json`. Commit it, since `bedrock_fw.json` records the commit hash. The rules are in
  [firmware_template_skill/SKILL.md](firmware_template_skill/SKILL.md) *Maintaining the script*.
- [CHANGELOG.md](CHANGELOG.md) at the root covers the Rust crates: entries under `## [Unreleased]` with `### Added`,
  `### Changed`, `### Fixed`, `### Removed`, short and user-facing, with the feature ID in parentheses. Mark breaking
  changes to the build-info format with **Breaking:** (deployed firmware carries it). A commit that bumps a crate
  version moves the `[Unreleased]` entries under a new `## [x.y.z] - YYYY-MM-DD` heading, so every version has its
  own section.

Questions like "what's new" or "what changed since X" are answered from these changelogs, newest sections first
(the user's version or date as the cutoff), with FEATURES.md for current status.

Pure refactors and typo fixes don't need an entry. The tpm repo's `/sync-repos` reads both files to log progress,
so a missing entry means work nobody sees.

## Layout

- `firmware_template_skill/` — `SKILL.md` (how agents use it), `scripts/bedrock_gen.py` (generator: `new`,
  `list-chips`, `chip-info`, `memory-x`, `hubris-memory`, `check-answers`, `compare`), `scripts/templates/` (Jinja2,
  conventions in its README), `scripts/data/` (`chips.toml` for non-STM32, `stm32.toml` HAL tables, `versions.toml`
  crate versions), `references/` (frameworks and tested matrix, memory layout, STM32 notes).
- `bedrock_build/` — build-script helper: embeds build info (WireWeaver-serialized, CRC, defmt-interned full form),
  `link_ram_cortex_m.x` for RAM linking.
- `bedrock_build_info/` — the build-info types (`BedrockBuildInfo`: crate, target, compiler, version control).
- `bedrock/` — host-side prototype: reads targets through probe-rs, ELF symbol scan (`nm.rs`).
- `bedrock_cli/`, `fw_registry/` — empty stubs (`println!("Hello, world!")`).
- `bedrock_template/` — the older cargo-generate template, superseded by `firmware_template_skill/` (FEATURES.md
  TPL-20).
- `book/` — notes on build info, counters and common issues.

WireWeaver crates are path dependencies on `../wire_weaver`. Fix WireWeaver problems there, with its own
FEATURES.md and CHANGELOG, rather than working around them here. The GUI side of the debug tool lives in RockFace;
items here are for the scriptable CLI and the shared crates.

## Commands

```sh
cargo build --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all
cargo test --workspace

S=firmware_template_skill/scripts/bedrock_gen.py
uv run $S list-chips
uv run $S chip-info --chip STM32H725IG
uv run $S new demo --chip STM32G0B1RE --counters --out <scratch>/demo      # generate into a scratch dir
uv run $S new --answers <fw>/bedrock_fw.json --out <tmp>                         # regenerate an existing firmware
```

Before declaring a template change done: regenerate and build the tested matrix in
`firmware_template_skill/references/frameworks.md#tested-matrix` (`cargo build`, plus `bootloader/` when enabled),
check every generated `.rs` with `rustfmt --edition 2024 --check`, and add the template CHANGELOG entry. Generate
into a scratch directory and delete it afterwards; never into one of the firmware repos.

## Conventions

- Generated firmware is ordinary code its owner edits freely. Template changes reach existing firmware only through
  the upgrade procedure (report first, apply what the user approves), never by overwriting files.
- New generator options get a sensible default and good `help`; renaming or removing one breaks `--answers` for
  existing firmware, so describe the mapping in the template changelog.
- Crate versions live in `scripts/data/versions.toml`; check crates.io before bumping.
- Firmware conventions the template encodes and every firmware keeps: flip-link, defmt with an explicit buffer
  size and level, `cnt` counters for unexpected events, fault handlers, BOOT pin reachable for the ROM bootloader.
- The build-info format is read by deployed firmware and host tools (RockFace): change it only compatibly, or bump
  its version and keep reading the old one.

## Tests

- Template: the build matrix above is the test. A bug fix in generated output adds the failing chip / option
  combination to the matrix.
- Rust crates: unit tests for the build-info encoding (round trip, CRC) and ELF parsing when touched.

## Commits

Conventional Commits with a scope: `feat(template): ...`, `fix(template): ...`, `feat(build_info): ...`,
`feat(bedrock): ...`, `docs: ...`, `build: ...`. Short imperative summary, blank line, body with what and why;
reference feature IDs (`feat(template): fw_info trait in generated firmware (DEV-2)`).

Commit on your own initiative (tpm CLAUDE.md "Commits are free, prod is gated"): work happens on a session branch
(`tpm work new SLUG`), each finished step is one commit with FEATURES.md and the right CHANGELOG updated in it. When
the work is done and the user agrees, `tpm land` puts it on main; pushing is the housekeeping timer's job. Anything
that reaches clients (prod deploy, firmware/OTA release, registry publish) still waits for the user's OK.

## Versions

Landing bumps the version, not each work commit (tpm CLAUDE.md "Landing bumps the version"), so any build from main
traces back to a release commit:
- Session commits add CHANGELOG entries under `[Unreleased]` without bumping. `tpm land` turns them into the next
  version's section with the manifest bump in one `release: x.y.z` commit: minor for Added/Changed/Removed/Deprecated,
  else patch; major only when the owner says so (`tpm land --version`).
- In a workspace, bump the changed crates by hand on the branch, then `tpm land --no-bump`. Docs-only, CI-only and
  no-behaviour-change refactors land without a bump.
- CLIs print version, git SHA and build time in `--version`, e.g. `tool 0.4.2 (a1b2c3d-dirty, built 3 Oct 2026
  18:20)`: a small `build.rs` without extra crates (`git rev-parse --short HEAD`, `-dirty` when
  `git status --porcelain` isn't empty, `rerun-if-changed` on `.git/HEAD` and `.git/index`, `unknown` without
  git). Firmware reports the same through `fw_info`. When touching a CLI that lacks it, add it.

The template has its own version in `firmware_template_skill/CHANGELOG.md`, bumped by the rule in *Two changelogs*.
