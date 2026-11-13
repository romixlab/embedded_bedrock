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
  changes to the build-info format with **Breaking:** (firmware and host tools read it). A commit that bumps a crate
  version moves the `[Unreleased]` entries under a new `## [x.y.z] - YYYY-MM-DD` heading, so every version has its
  own section.

Questions like "what's new" or "what changed since X" are answered from these changelogs, newest sections first
(the user's version or date as the cutoff), with FEATURES.md for current status.

Pure refactors and typo fixes don't need an entry. Our internal tooling reads both files to log progress,
so a missing entry means work nobody sees.

## Layout

- `firmware_template_skill/` — `SKILL.md` (how agents use it), `scripts/bedrock_gen.py` (generator: `new`,
  `list-chips`, `chip-info`, `memory-x`, `hubris-memory`, `check-answers`, `compare`), `scripts/templates/` (Jinja2,
  conventions in its README), `scripts/data/` (`chips.toml` for non-STM32, `stm32.toml` HAL tables, `versions.toml`
  crate versions), `references/` (frameworks and tested matrix, memory layout, STM32 notes).
- `bedrock_build/` — build-script helper: embeds build info (WireWeaver-serialized, CRC, defmt-interned full form),
  `link_ram_cortex_m.x` for RAM linking.
- `bedrock_build_info/` — the build-info types (`BedrockBuildInfo`: crate, target, compiler, version control).
- `bedrock_crash/` — crash capture (fault / panic record across reset, reset cause, boot counter); firmware takes it
  from git.
- `bedrock_led/` — status LED animation and driver (breath / blink, gamma, peak, identify; mono and RGB over
  embedded-hal PWM; `embassy` and `ww-indication` features); firmware takes it by path from the checkout next to it.
- `bedrock_sync/` — the shared clock (local clock disciplined onto UTC by host steer points or a sync line, holdover,
  slewing, scheduled actions, an optional oscillator backend, the local clock against USB frames in `sof.rs`;
  `embassy`, `ww-timebase` and `sof` features); host side is
  wire_weaver's `ww_clock_sync`. Firmware takes it by path, like `bedrock_led`, which aligns its animations to it.
- `bedrock_can/` — an FDCAN (M_CAN) channel as `ww_can_bus::Can` without a HAL (bit timing, configuration and
  filter checks against a `Controller` layout, frames to send, error state), from B135; the template's `--can`
  firmware and its host client use it. CAN FD lengths are exact_units' `OneOf<CanFdLengths>` (`FrameLen`), never a
  local table. Firmware takes it by path, like `bedrock_led`.
- `bedrock_usb/` — a USB device stack for STM32 fixed at compile time, full speed and high speed (FEATURES.md area `USBD`): shared
  types in `lib.rs`, the device as a `const` and its descriptors as `const fn` output (`desc.rs`), the usb_v4
  endpoint registers and USB SRAM layout with its RM0444 checks (`layout.rs`), `descriptors!` and `state!`
  (`macros.rs`), the EP0 state machine without registers (`control.rs`, chapter 9 host tests in
  `tests/chapter9.rs`), the usb_v4 driver (`v4/`: registers behind a `Regs` trait, ISR, EP0, typed endpoints, the
  simulated peripheral `sim.rs` its host tests run on, `embassy.rs` glue). Golden test `tests/donguru.rs` against
  embassy-usb's `Builder` on a dummy driver; `tests/v4_metapac.rs` holds the driver's registers to stm32-metapac.
  `ww.rs` (feature `ww`) is the WireWeaver class: `ww_function!` and `PacketSink` / `PacketSource` on the bulk
  endpoints. `device.rs` is `Usb::run()` over a `Backend` trait, the device on both (tests in `device/tests.rs`, on the
  simulated peripheral). The Synopsys OTG driver (`otg/`, H7 OTG_HS first): registers, the FIFO layout checked
  against the core, ISR, EP0, typed endpoints, the simulated core `sim.rs` and the bench `tests.rs` at full and high
  speed, `embassy.rs` glue (feature `embassy-stm32-otg`); `tests/otg_regs.rs` holds its registers to
  embassy-usb-synopsys-otg's `otg_v1`. `v3/` runs the usb_v4 driver on usb_v3 with 16-bit USB SRAM (L4, G4, WB): its
  own `Mmio` (half-word SRAM accesses, 16-bit registers), addresses, VDDUSB and embassy glue; `tests/v3_metapac.rs`
  holds it to the G474's metapac. `examples/g0b1_loopback` (usb_v4), `examples/g474_loopback` (usb_v3) and
  `examples/h725_loopback` (OTG_HS over ULPI) are standalone firmware crates with their own Cargo.lock, built by
  `just lint` at opt-level z and 3 and measured by `just usb-size`. Firmware takes it by path, like `bedrock_led`.
- `bedrock_clocks/` — exact clock planning on the host (FEATURES.md area `RCC`): a board's clock tree as exact
  rationals, checked against RM0468 / DS13311 / ES0491 / AN5337 (each limit in `h72x/limits.rs` names its section),
  encoded as register words and emitted as a reviewable `clocks.rs` that the board commits next to its
  `clocks.toml`. Golden test for B135 in `tests/`; `BLESS=1 cargo test -p bedrock_clocks` rewrites the golden file.
- `bedrock_clocks_rt/` — applies such a plan at boot (no_std, no PAC): `apply`, `verify`, `set_fracn` against a
  `Regs` trait, tested on a simulated chip with the golden `clocks.rs`. Firmware takes it by path, like `bedrock_led`.
- `bedrock_rom_boot/` — firmware updates through the STM32 ROM bootloader (G0, H7): boot word in a backup
  register, `early()` before the HAL (jump to system memory, reset after the ROM's leave, boot-loop rescue),
  reboots, ROM USB serial; `embassy` and `ww-dfu` features (`BootControl` / `UpdateNode` in place). Firmware takes
  it by path, like `bedrock_led`. RP firmware uses `Reboot` and the `dfu` module for BOOTSEL updates.
- `bedrock_board/`, `bedrock_board_cli/` — board infographics (`boardmap`: top and bottom views, power tree, pin
  table) from a `.kicad_pcb` and a TOML description, for the templates' `docs/board/`; FEATURES.md area `MAP`,
  example in the template's board preset `firmware_template_skill/scripts/data/boards/nucleo_g0b1re/`.
- `bedrock_lint/`, `bedrock_lint_cli/` — the firmware lint `bedrock-lint` (FEATURES.md area `LINT`): a firmware
  model (`model/`: manifests, cargo configuration, build script, memory.x and its `MEMORY` regions,
  bedrock_fw.json, syn facts, the linked ELF with frames and calls read from Thumb code), one file per check in
  `checks/` named by its catalogue id, waivers, baseline, text / JSON / SARIF reports. The catalogue
  `book/checks.md` is embedded at build time: a check's severity and title come from there, and a test holds every
  registered check to it and to a fixture under `bedrock_lint/tests/fixtures/` (a small firmware with an
  `expect.toml` of the findings it must give, and an `elf.toml` the test turns into a synthetic ELF for the elf
  checks: no binaries in git). A new check: a file in `checks/`, its `mod` line, a fixture. A check with a `src`
  and an `elf` part (STK-5, SIZ-5, PERF-3, ERR-3) is one check file with both.
- `bedrock/` — host-side prototype: reads targets through probe-rs, ELF symbol scan (`nm.rs`).
- `bedrock_usb_ch9/` — `bedrock-usb-ch9` (FEATURES.md `USBD-6`): nusb-based USB chapter 9 checks against any USB
  device (descriptors at a spread of wLength, halt and toggle on each bulk endpoint, SET_CONFIGURATION /
  SET_INTERFACE loops, bus resets), independent of `bedrock_usb`. Linux and macOS only (nusb's `Device`-level
  control transfers aren't available on Windows).
- `bedrock_cli/`, `fw_registry/` — empty stubs (`println!("Hello, world!")`).
- `bedrock_template/` — the older cargo-generate template, superseded by `firmware_template_skill/` (FEATURES.md
  TPL-20).
- `book/` — notes on build info, counters, blink codes and common issues; the firmware check catalogue `checks.md`
  (detail pages in `checks/`) with the review and bring-up checklists condensed from it (FEATURES.md area `LINT`).

Fix WireWeaver problems in WireWeaver, with its own FEATURES.md and CHANGELOG, rather than working around them
here (see *WireWeaver dependency*). The GUI side of the debug tool lives in RockFace; items here are for the
scriptable CLI and the shared crates.

## WireWeaver dependency

WireWeaver crates (and exact_units, by git without a rev, pinned by Cargo.lock) come from git only: the root
`Cargo.toml` names the source, `Cargo.lock` pins the commit. Nothing committed points at a sibling checkout, so a
`--locked` build gives the same result on every PC, with or without `../wire_weaver` next to this checkout, and at
whatever commit that sibling is (BLD-10).

- **Bump WireWeaver:** `cargo update -p wire_weaver` (moves every crate of the wire_weaver repository to its main
  together; `--precise SHA` for a given commit), build and test, commit `Cargo.lock` as `build: wire_weaver <sha>`.
- **Work on both repos at once:** `just dev-local` copies `.cargo/sibling-patch.toml` to the gitignored
  `.cargo/dev-local.toml`, which `.cargo/config.toml` includes, so builds take WireWeaver from `../wire_weaver`.
  Push the WireWeaver change first, then `just dev-git` (back to git), `cargo update -p wire_weaver`, commit the
  lock. Never commit `Cargo.lock` while dev-local is on: it names sibling paths, and the `--locked` gate refuses it.
- **A new WireWeaver crate:** add it to `[workspace.dependencies]` by git and to `.cargo/sibling-patch.toml`.
- **One source per crate:** `cargo tree -d` must not show wire_weaver, shrink_wrap, a `ww_*` crate or exact_units
  from two sources (CFG-4). exact_units has no `rev` here: Cargo.lock pins one commit, so after wire_weaver moves
  its exact_units pin, run `cargo update -p exact_units` and check `cargo tree -d`.

## Commands

```sh
cargo build --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all
cargo test --workspace
just usb-size        # bedrock_usb's examples (G0B1, G474, H725, F103): code per crate and per file, opt-level z and 3 (after just lint)
cargo run -p bedrock_lint_cli -- code-size <ELF> --by-crate   # code bytes per source file / crate (DWARF lines)

S=firmware_template_skill/scripts/bedrock_gen.py
uv run $S list-chips
uv run $S chip-info --chip STM32H725IG
uv run $S new demo --chip STM32G0B1RE --counters --out <scratch>/demo      # generate into a scratch dir
uv run $S new --answers <fw>/bedrock_fw.json --out <tmp>                         # regenerate an existing firmware
```

Before declaring a template change done: run `uv run $S matrix` (TPL-44, `firmware_template_skill/scripts/data/matrix.toml`)
to regenerate and build the tested matrix of `firmware_template_skill/references/frameworks.md#tested-matrix` into
a scratch directory it cleans up itself, check every generated `.rs` with `rustfmt --edition 2024 --check` and the
flip-link check (both part of `matrix`), and add the template CHANGELOG entry. A chip / option combination worth
tracking that `new` doesn't cover yet gets an entry in `matrix.toml` too. Never generate into one of the firmware
repos.

## Conventions

- Generated firmware is ordinary code its owner edits freely. Template changes reach existing firmware only through
  the upgrade procedure (report first, apply what the user approves), never by overwriting files.
- New generator options get a sensible default and good `help`; renaming or removing one breaks `--answers` for
  existing firmware, so describe the mapping in the template changelog.
- Crate versions live in `scripts/data/versions.toml`; check crates.io before bumping.
- Firmware conventions the template encodes and every firmware keeps: flip-link, defmt with an explicit buffer
  size and level, `cnt` counters for unexpected events, fault handlers, BOOT pin reachable for the ROM bootloader.
- Check the errata, always (our internal firmware rule, "check the errata, always"): template or firmware work on a
  chip's peripherals, clocks, flash, DMA, low-power modes or boot path reads the chip's errata sheet for the silicon
  revision in use, and for STM32 also http://www.efton.sk/STM32/gotcha/; errata are quoted from the document, not
  from memory. Where to find them per vendor and the errata the template already handles (ES0548 flash prefetch on
  dual-bank G0B1): `firmware_template_skill/references/errata.md`.
- Firmware lessons go into the check catalogue ([book/checks.md](book/checks.md)): a bug found on a board that a
  review, a lint or a bring-up step could have caught becomes a new entry (next free number of its category,
  severity, `check:` tier, the source repo and FEATURES id in the comment), and the review or bring-up checklist
  gets its ID when it belongs there. Template changes that act on a check name its ID.
- The build-info format is read by firmware and host tools (RockFace). It changes freely until something else
  depends on a specific shape of it; then treat it as a stable interface and follow normal compatibility rules.

## Tests

- Template: the build matrix above is the test. A bug fix in generated output adds the failing chip / option
  combination to the matrix.
- Rust crates: unit tests for the build-info encoding (round trip, CRC) and ELF parsing when touched.

## Commits

Conventional Commits with a scope: `feat(template): ...`, `fix(template): ...`, `feat(build_info): ...`,
`feat(bedrock): ...`, `docs: ...`, `build: ...`. Short imperative summary, blank line, body with what and why;
reference feature IDs (`feat(template): fw_info trait in generated firmware (DEV-2)`).

Commit on your own initiative (committing is free, only a release to clients is gated): work happens on its own
branch, each finished step is one commit with FEATURES.md and the right CHANGELOG updated in it. When
the work is done and the user agrees, it lands on main; pushing follows automatically. Anything
that reaches clients (prod deploy, firmware/OTA release, registry publish) still waits for the user's OK.

## Versions

Landing bumps the version, not each work commit, so any build from main
traces back to a release commit:
- Session commits add CHANGELOG entries under `[Unreleased]` without bumping. Landing turns them into the next
  version's section with the manifest bump in one `release: x.y.z` commit: minor for Added/Changed/Removed/Deprecated,
  else patch; major only when the owner says so.
- In a workspace, bump the changed crates by hand on the branch before landing without a bump. Docs-only, CI-only and
  no-behaviour-change refactors land without a bump.
- CLIs print version, git SHA and build time in `--version`, e.g. `tool 0.4.2 (a1b2c3d-dirty, built 3 Oct 2026
  18:20)`: a small `build.rs` without extra crates (`git rev-parse --short HEAD`, `-dirty` when
  `git status --porcelain` isn't empty, `rerun-if-changed` on `.git/HEAD` and `.git/index`, `unknown` without
  git). Firmware reports the same through `fw_info`. When touching a CLI that lacks it, add it.

The template has its own version in `firmware_template_skill/CHANGELOG.md`, bumped by the rule in *Two changelogs*.
