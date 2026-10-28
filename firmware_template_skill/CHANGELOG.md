# Changelog

Changes to the firmware template (generator, templates, data) that matter for generated firmware. Newest first.
Generated projects record the version and git commit they came from in `bedrock_fw.json`; an upgrading agent
reads the entries newer than that version (see the generated `AGENTS.md`).

Each entry: what changed, and **Upgrade notes** — how an existing firmware picks it up, which answers/options are
involved, what needs a hardware test. Add an entry (bump the version) for every change that alters generated output
or options; purely internal refactors only need a line under the next version.

## [Unreleased]

- CAN (TPL-53): `--can board|LIST|none` (default `none`) serves FDCAN channels as `can[]: ww_can_bus::Can` on
  embassy STM32G0 and H7 (needs `--wire-weaver`, `--log defmt`, `--counters`), ported from B135's firmware
  (`b135_fw/src/{can.rs,ww/can.rs}`, `b135_client/src/can.rs`).
  - `board` takes the preset's `[[can]]` tables (`instance`, `tx`, `rx`, `transceiver`, `standby`, `standby_active`
    `high|low`, H7 `fdcansel`); a list `FDCAN1:PD1,PB8,stby=PC13[:high|low],FDCAN3:PG9,PD12` works without a preset.
    Pins and instances are checked in stm32-data (TX / RX signals; the AF comes from embassy's pin traits). H7 takes
    two instances at most, each with half the message RAM (`bedrock_can::Controller::h7_half`; one instance gets
    `h7`, B135's layout); G0 has mcan's fixed RAM (3 / 3 / 3, no filter elements yet). CAN pins are left out of
    `--pins`. `--mcan-src git|PATH` [git] takes mcan from github.com/romixlab/mcan or a local checkout.
  - Firmware: `src/can.rs` (the CAN task on an `InterruptExecutor`, SWPMI1 / CEC / SPDIF_RX, whichever the chip has
    first; FDCAN interrupts one level below; RX and TX queues per channel, transmit credit, configure / filters /
    recover / abort commands, bus-off recovery, the transceiver's standby pin held inactive from boot; H7 runs the
    kernel clock off PLL2_Q at 80 MHz unless the preset's `fdcansel` names another, G0 off PCLK) and `src/ww/can.rs`
    (capabilities, configure, config, status, rx, tx, set_mode, recover, tx_abort, protocol_status, set_filters; the
    rest of `ww_can_bus::Can` is omitted). `firmware/Cargo.toml` gains `mcan` (`g0` / `h7`, `embassy`, `defmt`, no
    `rcc`), `bedrock_can` (from `--bedrock` like `bedrock_led`) and `ww_can_bus`; `src/ww.rs` the state, the
    `handler_model` entries and a `can_pump` arm in the server loop. Edge filtering stays off (ES0548 2.15.1 /
    ES0491 2.22.1), the Tx FIFO is used alone (2.15.2 / 2.22.2); ES0491 2.22.3 (DAR, `one_shot`) is not worked
    around (`references/errata.md`).
  - API crate: `ww_impl!(can[]: ww_can_bus::Can)` and the `ww_can_bus` dependency.
  - With `--host`: the client's `can` module (`Setup`, `CanRx`, `CanTx`, `can_*` methods, candump-style
    `frame_text`; host tests against `bedrock_can`), `<cli> can status|configure|send|dump [--index N]`, Python
    `can_configure`, `can_status`, `can_send`, `can_rx` and `examples/can_loopback.py`, and a README *CAN* section
    with an internal-loopback test (no bus, no transceiver).
  - Matrix: `g0can` (STM32G0B1CE, FDCAN1 PD1 / PB8, standby PC13) and `h7can` (STM32H725IG, FDCAN1 PB9 / PB8 and
    FDCAN3 PG9 / PD12).

  Upgrade notes: opt-in, nothing changes without `--can`. To add CAN to an existing firmware, regenerate it with its
  answers plus `--can ...` into a scratch directory and take `firmware/src/can.rs`, `firmware/src/ww/can.rs`, the
  `can` lines of `src/ww.rs`, `src/main.rs` (the clock mux, `can::start`) and `firmware/Cargo.toml`, the API's
  `can[]` (an additive API change: bump the API's minor version and `just api-save`) and, with `--host`, the client's
  `src/can.rs` and `src/can/`, the CLI's `can` command, the Python `can_*` functions and the README section. Check
  the transceiver's standby level against the schematic; nothing has run on hardware yet.

- The RP device suite, part 1 (TPL-25, TPL-55, TPL-56), for `--chip rp2040` / `rp2350` with `--wire-weaver` and
  without `--bootloader`:
  - Settings on RP flash: `--settings` writes `src/settings.rs` for embassy on RP (was region only): `ww_settings` on
    embassy-rp's blocking `Flash` through `BlockingAsync`, the region at the end of flash; main loads it and starts
    the LED at `led_at_boot`. An erase stalls XIP: embassy-rp runs it from RAM, core 0 only, core 1 paused.
  - `device.variant` on RP: `Variant { part, rev_id, rom_version, revision, package }` from SYSINFO CHIP_ID, the
    bootrom version byte and (RP2350) PACKAGE_SEL; `SiliconRevision` RP2040 B0..B2 / RP2350 A2..A4 by bootrom
    version, appendable as TPL-52; `Package` A (QFN-60) / B (QFN-80).
  - `device.boot` and `device.dfu.node` on RP (needs `--log defmt`): `reboot(Updater)` resets into BOOTSEL through
    the boot ROM (`src/boot.rs`), state always `Confirmed`, node `Profile::InPlace` from `bedrock_rom_boot::dfu`
    (the crate with no family feature). The USB serial number is `device.board`'s (the RP2350 ROM's in BOOTSEL).
  - With `--host`: client, CLI (`variant`, `boot`, `dfu`), Python and README for these items; texts name BOOTSEL
    instead of the STM32 ROM bootloader. Matrix: `rp2040h` gains `--settings`, new `rp2350h`.

  Upgrade notes: an RP `--wire-weaver` firmware gains API items (`variant`, `boot`, `dfu`; `settings` and `label`
  with `--settings`): additive, a minor API version bump and `just api-save`. Regenerate into a scratch directory and
  take `src/boot.rs`, `src/settings.rs`, the new parts of `src/ww/device.rs`, `src/ww.rs` (`serial_number`),
  `src/main.rs` (`boot::after_init`, the settings load), `Cargo.toml` (`bedrock_rom_boot`) and the API's `lib.rs`;
  with `--host` the client, CLI, Python module and README. Needs a board run: settings across a reset and power
  cut, `variant` on the board's silicon, `boot reboot --updater` then `ww dfu rp flash`.

- The `matrix` runs `just api-save` before a `--host` `--wire-weaver` entry's `cargo test`: since TPL-51 the
  evolution test fails on a fresh project without the current version's snapshot, so every such entry failed
  `host-test` (g0brd, g0board and the rest) on a PC without `ww` on PATH — `new`'s `post_steps` already runs
  `ww api save` at generation time when `ww` is there, saving the snapshot before the matrix gets to it. `just
  api-save` only refreshes the same snapshot in that case, so the fix is harmless either way. Upgrade notes: none,
  generated output is unchanged.

- `sort_uses` orders two `use a::{...}` groups of one path by their items, as rustfmt does: they kept the order the
  templates emitted them in, so a preset's clock `use` beside the HSI48 one (`embassy_stm32::rcc::{Hse, ...}` and
  `embassy_stm32::rcc::{Hsi48Config, mux}`) failed `cargo fmt --check`. Upgrade notes: none (`cargo fmt` fixes it).

- USB high speed through an external ULPI PHY (TPL-54): `--usb-phy ulpi` (default `fs`, the on-chip full-speed PHY
  as before) serves WireWeaver over `USB_OTG_HS` at high speed on STM32 with embassy release: the 12 ULPI pins come
  from stm32-data, a board preset's `[usb] ulpi = { NXT = "PH4" }` picks a pin where there are several (the
  generator notes each choice), `src/ww.rs` gets `usb_driver()` with the ULPI pins and `usb::Driver::new_hs_ulpi`,
  512-byte bulk packets, a 1024-byte OUT buffer and `UsbTimings::hs_higher_speed()`; with `--pins` the ULPI pins are
  left out. A preset's `[clock] usb = true` means its clock snippet sets the USB kernel clock itself (e.g. PLL3Q
  48 MHz): the generator then adds no HSI48 / CRS lines. The `ww::usb_driver(...)` call in `main` is one argument
  per line when it gets long (only the ULPI one does). Upgrade notes: none for FS firmware (unchanged output); a
  board with a ULPI PHY regenerates with `--usb-phy ulpi` in a scratch directory and takes `usb_driver()`, the
  packet length and the timings in `src/ww.rs` and the `usb_driver` call in `src/main.rs`.

- Several LEDs and RGB LEDs from the board preset (TPL-18): a preset's `[[leds]]` list (`name`, `pin` or
  `rgb = [R, G, B]`, `active = "high" | "low"`, `role = status | heartbeat | activity | power | fault | user`)
  replaces `[answers] led`. Each pin gets a timer channel from stm32-data (no channel used twice, plain CHy before
  CHyN, general-purpose timers before advanced ones, the time driver on another timer); `src/led.rs` drives them all
  through `bedrock_led::Indication<N>` (SimplePwm split into channels, ComplementaryPwm for CHyN outputs, active low
  inverted), each LED showing its role's bedrock look at boot (`LOOKS`, index constants per LED, `set_led`,
  `set_colour` for RGB). `led_on` / `led_off` drive the first LED, `leds.NAMES` lists them all and `OnlyEssential`
  keeps the power and fault LEDs. `--board` also takes a directory with a `preset.toml` (the matrix's test presets in
  `scripts/data/matrix_presets/`). A single mono, active-high status LED generates exactly what `[answers] led` did.
  Upgrade notes: none for existing firmware; a board with several LEDs moves them into `[[leds]]` and takes
  `src/led.rs`, `src/ww/leds.rs` and the LED lines of `src/main.rs` from a scratch directory.

- More board hardware from the preset (TPL-18): `[[buttons]]` (`name`, `pin`, `pressed = "low" | "high"`,
  `pull = "none" | "up" | "down"`) become `Input`s served as read-only bool properties under `buttons` in the API
  (`src/ww/buttons.rs`, `true` while pressed); `[[init]]` (`pin`, `level`, `what`) are driven right after
  `embassy_stm32::init` and held for good; `[bring_up]` (`use`, `code`, `takes`, `[bring_up.dependencies]`) is board
  set-up code placed after them in `main` with its crates in the firmware's `Cargo.toml`; `[usb]` `product` and
  `manufacturer` name the USB device. With `--pins` every one of these pins is left out. Every pin of the preset is
  checked to be used once. `[usb]` `vid` / `pid` are refused until wire_weaver TR-17 lets `usb_init` set them.
  Upgrade notes: none for existing firmware; a board that sets these up by hand can move them into its preset.

- `scripts/setup.py`: the module docstring is wrapped by the generator at 118 columns whatever the tool list is
  (it had lines of 122 and 134 on G0), and the four section rulers are 120 columns instead of 121. Upgrade notes:
  none needed, take the docstring from a scratch directory if you like.

- Silicon revisions that can grow (TPL-52): the API's `SiliconRevision` has `Unknown` first, then the errata sheet's
  revisions in its order, `ww_repr = unib32` (was `nib`) and the catch-all `Newer(u32)` (was `Other`), so a revision
  added to `stm32.toml` later is appended and an additive API change instead of renumbering `Unknown`.
  `stm32.toml` quotes each sheet's Applicability page, Table 2 "Device variants" (ES0548 Rev 4, ES0491 Rev 9, ES0392
  Rev 15, re-checked against the documents); ES0392's revision Y is marked "Y, W" as the sheet writes it. The matrix
  gains an `api-evolve` build (on `g0usb` and `h743`): one more revision at the end passes `ww api check`, one in
  front fails as breaking.

  Upgrade notes: a breaking change to the API for any firmware with `device.variant` (the discriminants move):
  regenerate into a scratch directory and take the `SiliconRevision` block of the API's `lib.rs`, bump the API's
  breaking version and `just api-save`. Host code that matched `Other` matches `Newer`, or `_`. Firmware code
  (`SiliconRevision::from_rev_id`) is unchanged. Take it together with the next breaking API change if there is one.

- API evolution check requires the current version's snapshot (TPL-51, wire_weaver EVO-12): `tests/evolution.rs`
  calls `wire_weaver_evolution::Check::require_current(true)` instead of `strict(true)`, so the test (and
  `just api-check`, CI, `.githooks/pre-commit`) fails until the version in the API crate's `Cargo.toml` has its own
  snapshot in `api_snapshots/` and the source matches it — the gap that let nucleo_g0b1re_template 0.1.5 ship API
  0.2.0 without `..._0_2_0.ron`. `just api-save` now saves (or refreshes) the current version's snapshot by running
  the same test with `WW_API_SAVE=1` (`cargo test -p <api> --test evolution --config "env.WW_API_SAVE.value='1'"`,
  works under the Windows `cmd.exe` shell too) instead of shelling out to `ww api save`, so saving needs no `ww`
  install. README "Changing the API", `AGENTS.md`, `scripts/rename.py` and the pre-commit hook's message updated
  to match. Upgrade notes: regenerate `<api>/tests/evolution.rs`, the root and `firmware/` `justfile`'s `api-save`
  recipe, `.githooks/pre-commit`'s message and README "Changing the API" from a scratch directory; run
  `just api-save` once to save the snapshot `require_current` now needs for the version already in `Cargo.toml`
  (a no-op if it was already saved) and commit `api_snapshots/` with it.

- `firmware/README.md`: the setup paragraph wraps at 116 columns (one line was 178 with probe-rs's installer), the
  memory table's backup-register row stays on one line with the boot word (it broke the table); the client's
  `tests/board_pins.rs` says to run `just board` when the rendered pin table is missing. Upgrade notes: none needed,
  take them if you like.

- Clock sync (TPL-40): `--clock-sync auto|on|off` (default `auto`: with `--wire-weaver` on embassy STM32), as
  nucleo_g0b1re_template's FW-11 / API-4 / HOST-6 / PY-3 (v0.1.7, a182dbc).
  - Firmware: `src/sync.rs` (the static `bedrock_sync::Sync`; over RTT it locks within 10 ms, over USB bedrock_sync's
    default 2 ms), `src/ww/clock.rs` (`ww_timebase::Clock` through `bedrock_sync::clock::ClockServer`, U32 µs, no
    sync line; `clock.info` names the time driver's timer), `mod sync` in `main.rs`, `clock` in `ServerState`,
    `clock.steer` in `handler_model` as async; with the LED breathing, `src/led.rs` runs `LEDS.run_with` on shared
    time. `firmware/Cargo.toml`: `bedrock_sync` (`embassy`, `ww-timebase`), `ww_timebase`, `ww_date_time`, `ww_si`'s
    `u32`; the `--ww-src` patch adds `ww_timebase` and `ww_si`.
  - API crate: `ww_impl!(clock: ww_timebase::Clock)`, `ww_timebase` dependency and features. Root workspace:
    `ww_timebase`, `ww_date_time`.
  - Host (`--host`): the client's `src/leds.rs` becomes `src/sync.rs` with `ClockCheck`, `clock_steering`,
    `wait_clock_locked`, `clock_check`, `clock_line` (and `ClockPhase` / `ClockStatus` re-exported); CLI `clock
    [--watch N] [--timeout S]`; Python `wait_clock_locked()`, `clock_check()`, `examples/clock.py`; README
    "LED and clock" (a preset's `readme-clock.md`: the measured residual on that board), FEATURES rows.
  - The G0 answers with `--board nucleo_g0b1re` regenerate all of G0's clock parts byte for byte.

  Upgrade notes: existing answers have no `clock_sync`, so they get `auto`: a `--wire-weaver` firmware on embassy
  STM32 gains `clock`. Regenerate into a scratch directory and take `src/sync.rs`, `src/ww/clock.rs` and the clock
  parts of `ww.rs`, `main.rs`, `led.rs`, both `Cargo.toml`s, the API's `lib.rs` and the host crates; or add
  `"clock_sync": "off"` to `bedrock_fw.json` to stay without it. `clock` is an addition to the API: bump its minor
  version, `#[since]` on the new item and `just api-save`. A firmware that took TPL-39's client `leds.rs` renames it
  to `sync.rs`. Check on the board: `<cli> clock --watch 30` locks within a few seconds.

- The LED breathes on its timer channel (TPL-39): `--led-mode auto|breath|gpio` (default `auto`: breath where the
  LED pin has a timer channel, embassy on STM32 so far), as nucleo_g0b1re_template's FW-10 / API-4 (v0.1.7, a182dbc).
  - The pin's `TIMx_CHy` (or `CHyN`) and AF come from stm32-data; `src/led.rs` drives it as `SimplePwm` (or
    `ComplementaryPwm`, duty inverted) at 1 kHz through embedded_bedrock's `bedrock_led` (`embassy` feature): a
    breath by default, `led_on` / `led_off` hold it solid on / off, `led_at_boot` starts it solid on. `main.rs`
    spawns `led::led_task` instead of keeping an `Output`.
  - embassy-stm32's time driver is named, `time-driver-timN` (TIM4, TIM3, ... minus the LED's timer), never
    `time-driver-any`, which may take the LED's timer.
  - With `--wire-weaver`: `leds: ww_indication::Indication` in the API (mode, brightness, identify), served by
    `src/ww/leds.rs`; `ww_indication` and `ww_si`'s `u32` feature in the firmware, `ww_indication` in the API crate
    and the root workspace, `ww_indication` in the `--ww-src` patch. With `--host`: client `src/leds.rs`
    (`leds_mode`, `set_leds_mode`, `brightness`, `set_brightness`, `identify`), CLI `leds show|mode|brightness|identify`,
    Python `leds_mode`, `brightness`, `identify()`, README "LED", FEATURES rows.
  - No timer channel on the pin, another framework, RP / nRF / ESP: the plain GPIO output as before (a note from
    `new`); `--led-mode breath` refuses there, `--led-mode gpio` keeps the old output everywhere.

  Upgrade notes: existing answers have no `led_mode`, so they get `auto`, and on STM32 embassy an LED pin with a
  timer channel now breathes. Regenerate into a scratch directory and compare `Cargo.toml` (the time driver: check
  nothing else in the firmware uses that timer; `bedrock_led`), `src/led.rs`, `src/main.rs` (the LED task replaces
  the `Output`), `src/ww.rs`, `src/ww/leds.rs` and the API crate; or add `"led_mode": "gpio"` to `bedrock_fw.json`
  to keep the plain output. `leds` is an addition to the API: bump its minor version, `#[since]` on the new item and
  `just api-save`. Host crates: `leds.rs` in the client and the CLI / Python additions. Check on the board: the LED
  breathes, `led on|off` hold it, `leds identify` blinks it.

- `--pins board|LIST|none` (TPL-46), default `none` (the `nucleo_g0b1re` preset answers `board`): the firmware serves
  `pins` (`ww_pin_diag::PinDiag`) beside the device suite, as nucleo_g0b1re_template FW-12 / TPL-11 (467f889) does.
  `board` takes every pin the preset's board map marks free, named as there, plus the preset's `[pins.read_only]`;
  a list `PA0,PB3,PC13:ro` works without a preset. Never the SWD pins, the LED or USB DP / DM. Embassy on STM32 with
  `--log defmt` (RP / nRF: TPL-48, the HAL crates: TPL-49).
  - Firmware: `src/pins.rs` (a pin stays analog until a host sets or reads it; `release_all` and every link drop put
    it back), the `PinDiag` impl in `src/ww.rs` (refusals counted with `cnt` when `--counters`, release on
    `LinkEvent::Down`), `Point` / `pins_points` in `src/ww/device.rs`, `mod pins` and `Pins::new` in `main.rs`,
    `ww_pin_diag` and `ww_gpio` in `firmware/Cargo.toml`.
  - API crate: `ww_impl!(pins: ww_pin_diag::PinDiag)`, `ww_pin_diag` / `ww_gpio` dependencies.
  - Host: `just pins` (`ww pins tui`: over RTT `--rtt CHIP --probe`, over USB `--api`, with the board map as preset
    when there is one), README "Pins" (from a preset also its `readme-pins.md`: the board's headers and a jumper
    test); with `--host` the root workspace and the client take `ww_pin_diag` / `ww_gpio`, a preset's pin test plan
    goes to `docs/board/`, and with a pin table `<name>_client/tests/board_pins.rs` checks the list against it.
  - The G0 answers with `--board nucleo_g0b1re` regenerate G0's `firmware/src`, `firmware/Cargo.toml`, the client
    test, the pin test plan and the README section byte for byte.

  Upgrade notes: new answer `pins` (missing = `none`, nothing changes). To add it: regenerate with `--pins` into a
  scratch directory and take `src/pins.rs` and the `pins` parts of `ww.rs`, `ww/device.rs`, `main.rs`, both
  `Cargo.toml`s, the API's `lib.rs`, the justfile and README; adding `pins` to the API is a minor version bump of
  the API crate (`just api-save` after it; `ww api save` 0.5.0 of 22 Oct panics on a `PinDiag` API, a newer `ww`
  is needed). Pins the firmware uses for something else must leave the list first. `just pins` needs a `ww` with
  `pins tui` (wire_weaver TOOL-21, ab32916). Run `ww pins test` on a board before relying on it.

- `--host`: the Python crate's lib has `test = false` (it is tested from Python with `just test-py`; `cargo test --workspace` no longer runs a Rust test binary that needs libpython at run time), and the CLI's `use` of the client wraps at rustfmt's 98 columns, not 100 (failed `rustfmt --check` for longer project names). Upgrade notes: add `test = false` under `[lib]` in `<name>_py/Cargo.toml`.
- Tested matrix: `f103m` builds release only (TPL-34).

- The template suite, host crates (TPL-37): `--host` generates `<name>_client`, `<name>_cli` and `<name>_py` next to
  the API crate, as nucleo_g0b1re_template (cc1c30b) has them.
  - `<name>_client`: the typed `<Board>` over the generated client with one `Error`, methods for the device items
    the answers have (firmware, last reset, board, uid, label, counters and the BKP buffer, log, variant,
    settings / LED at boot, boot, dfu, report). Target: with RTT `Probe` (chip, `<NAME>_PROBE` VID:PID from
    `--probe`, serial, ELF for a fast attach), with USB `Usb` (VID:PID `USB_ID`, serial). Counters and the log are
    decoded with the firmware's ELF, found by its build id (cnt_core).
  - `<name>_cli` (`--cli-name`): clap with anstream colour, dynamic completion (probe serials, or USB devices through
    nusb), `--version` with git SHA and build time (`build.rs`); `info`, `led`, the device subcommands, `probes` or
    `devices`.
  - `<name>_py`: PyO3 + maturin, a uv project (`uv lock` in `new`), `Device(...)`, examples `blink.py` and
    `device.py`, a pytest (`just test-py`).
  - Root `Cargo.toml` gains the members and `[workspace.dependencies]` for them; host crate versions in
    `versions.toml`. The G0 answers with `--board nucleo_g0b1re` regenerate G0's host crates but for wording and
    reflow (title "Nucleo-G0B1RE", the stm32.toml variant table with revisions Z and Y).

  Upgrade notes: existing firmware with `host` on and no host crates: regenerate into a scratch directory and copy
  `<name>_client/`, `<name>_cli/`, `<name>_py/` in, add them to the root workspace's members and dependencies, run
  `cargo generate-lockfile` and `uv lock` in `<name>_py/`. A firmware that already has its own host crates (G0)
  compares them and takes what it wants; nothing changes for firmware without `--host`. The client's methods need
  the device suite (TPL-41) in the API. Run `info`, `log -f` and `counters` once against the board.

- The device suite (TPL-41): every `--wire-weaver` firmware serves the device contract as nucleo_g0b1re_template
  does, with the items the answers allow.
  - API crate: `device: Device` with `firmware`, `last_reset` (crash capture), `board` (name, revision, serial from
    the preset or the project), `uid` (STM32 96-bit id; RP2040 the flash chip's id, RP2350 OTP chip id, nRF FICR
    DEVICEID), `label` and `settings` (`--settings`: `Settings` with `led_at_boot`), `counters` (`--counters`), `log`
    (`--log defmt`), `variant` (STM32 G0B1 / G0C1, H72x / H73x, H742 / H743 / H753 / H750: DBGMCU_IDCODE, FLASH_SIZE
    and a `SiliconRevision` table from the errata sheet), `boot` and `dfu` (below). Version 0.1.0 for new projects.
  - Firmware: `src/ww.rs` serves it (`ServerState::new` takes the last reset, the settings and on RP2040 the uid;
    a `select` over requests, the log pump and the settings write), `src/ww/device.rs` the `device` handlers,
    `src/log.rs` (`--log defmt`: `ww_defmt_logger` store, `device.log.frames` stream, and with RTT transport the
    defmt RTT tee that returns `ww_up` / `ww_down`), `src/settings.rs` gains `led_at_boot` (slot 1, the LED's state at
    boot) and `reset_to_defaults`. The log's ring and store are 4 / 8 KiB, 1 / 2 KiB on parts with less than 64 KiB
    of RAM.
  - `boot` and `dfu.node` through `bedrock_rom_boot` (G0, single-core H7; needs `--log defmt`, not with
    `--bootloader`): `src/boot.rs` (`boot::early()` first thing in `main`, the boot word in the last backup
    register, the boot-loop rescue), `memory.x` and `CNT_BKP_BUFFER_SIZE_WORDS` leave that word out of cnt's BKPSRAM.
    With `--ww-src` a local checkout, the `[patch]` also takes `ww_dfu` from it.
  - `ServerState::new` on one line when it fits (rustfmt).

  Upgrade notes: **breaking for the API** of existing `--wire-weaver` firmware with the flat layout (`firmware`,
  `last_reset`, `label` at the root of the API trait): they move under `device` (`device.firmware`, ...), so bump the
  API crate's version 0.1 -> 0.2, keep the 0.1 snapshot in `api_snapshots/` and run `just api-save`; host code that
  calls the old paths follows. Regenerate with the firmware's answers into a scratch directory and compare
  `<api>/src/lib.rs`, `src/ww.rs`, `src/ww/device.rs`, `src/log.rs`, `src/boot.rs`, `src/settings.rs`, `src/main.rs`,
  `memory.x` and `Cargo.toml`; take the `device` items the firmware wants, keep its own methods. Settings: the
  layout gains `led_at_boot` (slot 1): a firmware that already uses slot 1 picks another slot. ROM boot (G0 / H7
  without `--bootloader`): the boot word takes the last backup register, so cnt's BKP counters lose one word;
  test `dfu` and the boot-loop rescue on the board. A firmware made from nucleo_g0b1re_template (cc1c30b) with
  `--board nucleo_g0b1re` regenerates to its `firmware/src` byte for byte.

- The template suite, first part (TPL-37 layout, TPL-38, TPL-43, TPL-45): what nucleo_g0b1re_template proved, from
  the generator.
  - `--wire-weaver` projects get a host workspace at the root (`Cargo.toml` with the API crate as member, the
    firmware excluded, `Cargo.lock` from `cargo generate-lockfile` in `new`); the API crate loses its own
    `[workspace]`. Fixes the missing lockfile `#[ww_api_root]` needs with `--ww-src git` (TPL-43).
  - API evolution check (TPL-38): the API crate's dev-dependency `wire_weaver_evolution` and `tests/evolution.rs`
    (strict: the current version needs a snapshot in `api_snapshots/`), recipes `api-check`, `api-save`, `hooks`,
    `test`, `.githooks/pre-commit` and `scripts/hooks.py` (chained after the hooks already in `.git/hooks/`, never
    `core.hooksPath`), and a hooks step in `scripts/setup.py`.
  - `--probe VID:PID` (TPL-45): `.cargo/config.toml`'s runner (`probe-rs run --probe`, `cnt run --probe`) and every
    probe-rs / cnt recipe select the probe by it; the serial is appended from `<CLI>_PROBE` (environment or `.env`,
    `set dotenv-load`). Default: the board preset's, else the family's on-board probe (ST-LINK 0483:374b, RP's
    CMSIS-DAP 2e8a:000c, which replaces the udev needle `cmsis-dap` as the id; J-Link OB 1366:1015 on nRF).
  - `--board PRESET` (TPL-45): `scripts/data/boards/<preset>/`, answers plus the board's clock tree (replaces the
    clock TODO in `main.rs`), README board section, `docs/board/` and logo values. First preset `nucleo_g0b1re`.
  - `--host` (TPL-37, needs `--wire-weaver`): root `justfile` for the whole project, `.github/workflows/ci.yml` and
    `setup.yml` (actions/checkout@v5), `scripts/rename.py`, README, AGENTS (layout and procedures, then the upgrade
    section), FEATURES and CHANGELOG skeletons, LICENSE (MIT, holder vhrd.tech unless the preset names one), `docs/logo/` (Typst;
    `new` renders it when `typst` is on PATH) and `docs/board/` with `--board` (rendered when `boardmap` is on
    PATH). `--cli-name` names the CLI and the env prefix. The client, CLI and Python crates follow in the next
    entry.
  - embassy-stm32 `main.rs`: `let config` without `mut` when nothing changes it (an `unused_mut` warning failed
    `just lint`).

  Upgrade notes: existing `--wire-weaver` firmware: add the root `Cargo.toml` (regenerate and compare), drop the
  `[workspace]` table from `<api>/Cargo.toml`, move or create `Cargo.lock` at the root (`cargo generate-lockfile`),
  add `tests/evolution.rs` and the dev-dependency, run `just api-save` once to record the current API, then
  `just hooks`. Answer `probe` (new): the board's probe as `VID:PID`; take it into `.cargo/config.toml`'s runner
  and the recipes, and put the serial in `.env` (`<CLI>_PROBE=VID:PID:SERIAL`, not committed) on PCs with more than
  one probe. `host`, `board`, `cli_name` (new answers) default to off / none: say yes only when the firmware wants
  the host side or matches a preset; the files `--host` adds that a project already has (README, FEATURES,
  CHANGELOG, LICENSE) are its own: compare, never overwrite. A firmware made by hand from nucleo_g0b1re_template
  (`--board nucleo_g0b1re`) should regenerate to the same files apart from the host crates and the device suite,
  which arrive in later entries. Nothing on the target changes.

- Internal: the tested matrix of `references/frameworks.md#tested-matrix` is now data (`scripts/data/matrix.toml`)
  and a `matrix` subcommand that generates, builds, rustfmt-checks and flip-link-checks every entry (TPL-44). Each
  entry builds in its own cargo build dir under `~/.cache/bedrock_matrix/`, removed when the entry is done (a
  shared `build-dir` kept one per scratch path: two full runs filled a 555 GB disk). No generated output changed.

- `just setup` in every generated project (TPL-36): `scripts/setup.py` (run with uv, stdlib only) installs what the
  firmware needs on a fresh Arch, Ubuntu / Debian or Windows PC: rustup with the toolchain, targets and components of
  `rust-toolchain.toml` (Xtensa: espup), probe-rs, and as the answers need flip-link, espflash, cnt, ww. Steps that
  need root (system packages, the probe's udev rule, the ST-LINK driver on Windows) are printed, `--root` runs them;
  `--check` reports what is missing. It prints its plan first, naming every script it fetches and runs (rustup's and
  probe-rs's installers, probe-rs's udev rules from a pinned commit). The `justfile` is now generated for every
  target (ESP too), at the output root, with the `setup` recipe; with `--wire-weaver` its `flash-bootsel` (RP) and
  `cnt` recipes build with `cd firmware`, so they move from `firmware/justfile` to the root. The generated README's
  *Toolchain* section says `just setup`.

  Upgrade notes: take `scripts/setup.py` and the `setup` recipe as they are (no answers involved); a firmware with
  its own `justfile` adds the recipe. With `--wire-weaver`: move `firmware/justfile`'s recipes to the root
  `justfile` (they gain `cd firmware && `). Nothing on the target changes.

- `--build-info` builds with `--bedrock git` (BLD-8): embedded_bedrock now takes WireWeaver from git instead of
  `../wire_weaver`, so `bedrock_build` from git no longer fails with "no matching package named `wire_weaver`". With a
  local `--ww-src`, `Cargo.toml` and `bootloader/Cargo.toml` get a `[patch."https://github.com/vhrdtech/wire_weaver"]`
  block pointing bedrock_build's `wire_weaver`, `ww_date_time` and `ww_version` at that checkout. The bootloader's
  `build.rs` embeds `compact()` alone without its `defmt` feature (`bedrock_build::serialize_compact_build_info`),
  which failed to compile before. `bedrock_build` is on `build-info-common` 0.0.46, matching `build-info-build`
  (mismatched `BuildInfo` types before). New tested-matrix entries `g0bi` (local sources) and `g0bigit` (git
  sources, once per release).

  Upgrade notes: firmware with `"build_info": true` takes the new `bedrock_build` on `cargo update -p bedrock_build`
  (git) or at once (path). With a local `ww_src`, add the `[patch]` block (regenerate `Cargo.toml` and compare);
  without it the build has two WireWeavers, which works but builds twice. A bootloader with build info takes the
  `CARGO_FEATURE_DEFMT` branch in `build.rs`. Nothing to do for firmware without build info.

- `cnt run` as an alternative runner (TPL-35): `.cargo/config.toml` (every non-ESP family) carries a commented
  `runner = "cnt run --chip <CHIP>"` under the active `probe-rs run` one, and the generated `justfile` (now written
  for every non-ESP family, not just RP, TPL-27) gets a `cnt` recipe (`cargo build && cnt run`): both flash, reset
  and show the defmt log plus live counters as they change, without switching the default runner. `cnt` bumped to
  0.5 in `data/versions.toml` (macro API and on-target buffer format unchanged from 0.4).

  Upgrade notes: regenerate to pick up the commented runner line, the `justfile`'s `cnt` recipe and the `cnt` crate
  bump; an existing firmware can also add the line and recipe by hand. `cargo install cnt_cli` (0.5) on the host.

- `flash-bootsel`'s generated `justfile` recipe uses `ww dfu rp flash` (TPL-27): wire_weaver renamed `ww bootsel`
  under `ww dfu` (the old name still runs as a hidden alias for now). `references/frameworks.md` and the generated
  `ww/README.md.j2` updated to match.

  Upgrade notes: regenerate the `justfile` recipe, or edit it by hand (`ww bootsel` -> `ww dfu rp`); either name
  works until wire_weaver drops the alias.

- `--embassy release|main` (TPL-28), default `release`: `main` takes every embassy crate the project uses from
  embassy-rs/embassy main at the commit in `data/versions.toml` (`embassy-main`, 262fdba545 of 19 Oct 2026) through a
  `[patch.crates-io]` block in `Cargo.toml` and `bootloader/Cargo.toml`, the dependency lines staying the crates.io
  ones; the code then uses main's API (`mux::Usbsel::Hsi48`, `usb::Driver::new(usb, dp, dm, Irqs)`,
  `Driver::new_fs(usb, dp, dm, Irqs, buf, config)`). `release` prints the same block commented out, ready to switch.
  STM32 with `--framework embassy` only (RP / nRF / ESP: TPL-29). With `--wire-weaver` on a USB v1 / v4 part (F1, F3,
  L1, C0, G0, H5, U0, U3, U5) `src/ww.rs` calls `usb_init_with_endpoints` with double-buffered bulk endpoints
  (embassy-rs/embassy#7275, ~2x throughput behind a hub on Donguru); this needs WireWeaver with
  `usb_init_with_endpoints` (wire_weaver_usb_embassy, TR-12).

  Upgrade notes: new answer `embassy` (missing = `release`, nothing changes but the commented-out block at the end
  of `Cargo.toml`). A firmware already on embassy main by hand (Donguru: `[patch.crates-io]` on 262fdba, main's API
  in its own code) records `"embassy": "main"`; take the generated block only if its crate list or rev differs.

- G0 flash prefetch and the ES0548 check (TPL-32): with `--framework embassy` on STM32G0, `main.rs` turns
  `FLASH_ACR.PRFTEN` on after `embassy_stm32::init` (embassy-stm32 0.6 sets the wait states but leaves prefetch off,
  8-16% slower code at 64 MHz; embassy main does it itself). On dual-bank G0B1 / G0C1, `memory.x` gets
  `ASSERT(__etext <= 0x08040000, ...)`: ES0548 2.2.10, "the code prefetch may fail upon branching and function calls
  across flash memory banks", not fixed in any revision, so code reaching bank 2 now fails to link instead of
  HardFaulting at run time (seen on Donguru, its FW-38).

  Upgrade notes: G0 firmwares get the three `PRFTEN` lines in `main.rs` (embassy 0.6) or a comment (main), and on
  G0B1 / G0C1 the ASSERT in `memory.x`. A firmware whose code already spans both banks (Donguru: ~400 KiB) must keep
  prefetch off and leave the ASSERT out: reject both, note it in `nuances`. Check after taking it: the firmware
  links (the ASSERT holds), and runs.

- Errata (TPL-33): `new` prints where the chip's errata sheet is (STM32: the `errata_sheet` link of stm32-data plus
  http://www.efton.sk/STM32/gotcha/; RP, nRF, ESP: the vendor document), also in the generated `README.md` notes.
  `references/errata.md` lists them per vendor and the errata the template handles.

  Upgrade notes: README.md gains one note line; nothing else.

- `src/last_reset.rs` uses `Second::milli` (ww_si 0.2 removed `Second::milliseconds`), so `--wire-weaver` projects
  build against today's WireWeaver again.

  Upgrade notes: take the one-line change in `last_reset.rs` (firmware on ww_si 0.2 already has it, e.g. Donguru).

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
