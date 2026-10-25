# Frameworks, families, versions

## Crate versions (verified on crates.io, see `V` in the script)

embassy-executor 0.10 · embassy-time 0.5.1 · embassy-sync 0.8 · embassy-stm32 0.6 · embassy-rp 0.10 · embassy-nrf 0.11 ·
embassy-boot-{stm32 0.8, rp 0.10, nrf 0.12} (all on embassy-boot 0.7) · esp-hal 1.2 · esp-rtos 0.4 · esp-println 0.18 ·
esp-backtrace 0.20 · esp-bootloader-esp-idf 0.6 · stm32-hal2 2.1 ·
stm32-rs HALs: stm32f0xx 0.18 · stm32f1xx 0.11 · stm32f3xx 0.10 · stm32f4xx 0.23 · stm32f7xx 0.8 · stm32g0xx 0.2 ·
stm32g4xx 0.1 · stm32h7xx 0.16 · stm32l0xx 0.10 · stm32l4xx 0.7.1 · cnt 0.5 · defmt 1.1 · defmt-rtt 1.3 · panic-probe 1.0 ·
rtt-target 0.6 · cortex-m 0.7 · cortex-m-rt 0.7 · build-info-build 0.0.46.

Pitfalls with older documentation / examples:
- `embassy-executor` `task-arena-size-*` and `nightly` features: gone since 0.10, tasks are statically allocated on
  stable → no nightly needed, no `#![feature(impl_trait_in_assoc_type)]`.
- `cnt 0.5`: `cnt!(name: u32)` counts unconditionally, `cnt_if!(cond, name: u32)` conditionally (0.2 needed
  `cnt_if!(true, name: u32 += 1)`). Needs Rust 1.88+, host `cnt_cli` 0.5 (format version 2, same as 0.4; rejects 0.2/0.3
  firmware).
- `embassy-stm32`: prefer `time-driver-any` over `time-driver-timN`. `SMPSSupplyVoltage::V1_8`/`V2_5`.
- `bedrock_build` build-info is optional (`--build-info`); `bedrock_build_info` depends on WireWeaver crates from git
  (`ww_date_time`, `ww_version`), not on crates.io.

## Per family

### STM32 (embassy)
`embassy-stm32` features: `<chip>`, `unstable-pac`, `exti`, `time`, `time-driver-any`, `defmt`, `chrono` with `--rtc`.
`cortex-m` `critical-section-single-core`. thumbv6m adds `portable-atomic` with `critical-section`.
Bootloader: `Flash::new_blocking(p.FLASH).into_blocking_regions().bank1_region`, `BootLoader::prepare::<_,_,_,ERASE>`
with the largest erase size, `bl.load(BANK1_REGION.base() + active_offset)`.

Flash prefetch on G0: embassy-stm32 0.6 leaves `FLASH_ACR.PRFTEN` off, so `main.rs` sets it after
`embassy_stm32::init` (embassy main does it itself, embassy-rs/embassy#7276). On dual-bank G0B1 / G0C1 `memory.x` gets
`ASSERT(__etext <= <bank 2 base>)`: prefetch faults on branches across the banks (ES0548 2.2.10, see `errata.md`).

#### embassy main (`--embassy main`, TPL-28) {#embassy-main}

`--embassy main` takes every embassy crate from embassy-rs/embassy main at `embassy-main` in `data/versions.toml`
(262fdba545, 19 Oct 2026) through `[patch.crates-io]` in `Cargo.toml` (and `bootloader/Cargo.toml`), listing only
the crates the project uses (cargo warns about unused patches). The dependency lines stay the crates.io ones: main
still has those versions. With `--embassy release` (default) the same block is printed commented out. What main has
that 0.6 hasn't: double-buffered USB bulk endpoints on USB v1 / v4 (#7275; with `--wire-weaver` `src/ww.rs` passes
them to wire_weaver_usb_embassy's `usb_init_with_endpoints`), G0 flash prefetch in `rcc` (#7276). API differences the
templates handle: PAC enums in CamelCase (`mux::Usbsel::Hsi48`), the interrupt binding last in `usb::Driver::new` /
`new_fs`. Others a firmware may meet: `uid::uid()` returns `&'static [u8; 12]`, one ADC driver for every chip
(`Adc::new_blocking(adc, Config)`, `Config.oversampling`, `into_ring_buffered(.., sequence, None)`; SMPSEL and CCRDY
handled inside), `embassy-net-driver` is gone (xarxa). STM32 only so far; RP / nRF on main: TPL-29.

### RP2040 / RP235x (embassy)
`embassy-rp` features `rp2040|rp235xa|rp235xb`, `time-driver`, `critical-section-impl`, `executor-thread`,
`executor-interrupt`, `binary-info` (235x). embassy-rp brings its own executor and `__pender`, therefore
`embassy-executor` gets **no** `platform-cortex-m/executor-*` features and main is
`#[embassy_executor::main(executor = "embassy_rp::executor::Executor", entry = "cortex_m_rt::entry")]`.
`cortex-m` with `inline-asm` (no `critical-section-single-core`, dual core). rp2040 needs `-Tlink-rp.x` (boot2 section)
and the `BOOT2` region; embassy-rp provides boot2 (`boot2-w25q080` default) and the RP235x `IMAGE_DEF` block
(`imagedef-secure-exe` default) — the `.start_block/.bi_entries/.end_block` sections are appended to `memory.x`.
Flash size defaults to 2M (rp2040/rp2354) / 4M (rp2350); override with `--flash-size`.
Bootloader uses `WatchdogFlash::<FLASH_SIZE>` (8 s) and `embassy_rp::flash::FLASH_BASE`.
probe-rs chips: `RP2040`, `RP235x`. Alternative runner: `ww dfu rp flash` (wire_weaver CLT-21, over PICOBOOT in
the RP2040/RP2350 boot ROM, BOOTSEL mode, no probe) — takes the ELF straight, no `picotool`/`elf2uf2-rs` step; the
template's `just flash-bootsel` recipe wraps it.

### nRF52 / nRF91 (embassy)
`embassy-nrf` features `nrf52840` / `nrf9160-s` / `nrf9151-s` (secure), `time-driver-rtc1`, `gpiote`, `unstable-pac`, `time`.
Flash starts at 0 → no symbol shifting. nRF91: `RAM` at 0x20010000 (first 64K reserved as `IPC` for the modem),
nrf9151 runner adds `--allow-erase-all`. Bootloader uses `Nvmc` + `WatchdogFlash`, `bl.load(active_offset)`.
Softdevice layouts are not generated (adjust `FLASH`/`RAM` origin manually).

### ESP32 (embassy via esp-rtos, or bare esp-hal)
`esp-hal` `<chip>`, `unstable`; `esp-rtos` `<chip>`, `embassy` (provides `#[esp_rtos::main]`, `esp_rtos::start(timer, FROM_CPU_INTR0)`);
`esp-bootloader-esp-idf` + `esp_app_desc!()` mandatory for the esp-idf bootloader; `esp-backtrace` `panic-handler`;
`esp-println` `defmt-espflash` (defmt over UART/USB-serial-JTAG decoded by `espflash --log-format defmt`) or `log-04`.
`--log rtt` works with probe-rs on chips with a debug interface (C3/C6/S3/H2), then swap the runner.
`build-std = ["core"]` is required, stable toolchain for RISC-V, `channel = "esp"` (espup) for Xtensa (`esp32`, `esp32s2`, `esp32s3`);
Xtensa needs `source ~/export-esp.sh` and `-C link-arg=-nostartfiles`. `-Tlinkall.x` must be the last linker script.
Default LED pins per chip in `ESP_LED`.

### stm32-hal2 (`--framework stm32-hal2`)
See `stm32-notes.md`. Blocking `#[entry]` skeleton, `hal::pac`, no bootloader integration (embassy-boot still usable via
`embedded-storage` traits, code must be added manually).

### stm32-rs HALs (`--framework stm32xx-hal`)
One crate per series (`stm32f0xx-hal` ... `stm32l4xx-hal`), renamed to `hal` in `Cargo.toml`; table `[xxhal.<series>]` in
`data/stm32.toml` (chip key → features, placeholders `{size}`, `{density}`, `{mcu}`). Blocking `#[entry]` skeleton;
RCC/GPIO setup differs per series (`app/src/main/stm32xx-hal.rs.j2`), all keep the reset clock (HSI) with a TODO.
Quirks: F0/F3/G0/L0 are still on embedded-hal 0.2 (`toggle().ok()`, F0 pins need a `cortex_m::interrupt::free` cs);
F1 needs `crl`/`crh`; F1 density (`medium`/`high`/`xl`) and F0/F3 size suffixes (`stm32f303xc`) come from the flash
size code; L0 needs `mcu-<package>` (first stm32-data package the crate knows — check it) and `disable-linker-script`
(we generate `memory.x`); H7: `--supply-config` maps to `Pwr::ldo()/smps()/bypass()/smps_{1v8,2v5}_feeds_ldo()`,
`SMPSExternalLDO*` is not supported by the crate; revision-V features (`stm32h743v`), H745/H755/H757 use `stm32h747cm7`.
`defmt` feature only on F1/F4/G4. Not covered: G0B1/G0C1/G05x/G06x, C0, H5, U5, WB, WL (use embassy or stm32-hal2).
Several crates are stale (f0 2021, g0 2023, l0 2022, l4 2022); prefer embassy for new designs.

### bare (`--framework bare`)
`cortex-m-rt` `#[entry]` + `wfi` loop (or `esp_hal::main` on ESP) — for adding a PAC/other HAL/RTIC by hand.
All the infrastructure (memory.x, config, counters, logging, init_ram) is still generated.

## Logging

| `--log` | crates | notes |
|---|---|---|
| defmt | defmt, defmt-rtt, panic-probe (`print-defmt`) | `DEFMT_LOG="<lvl>,<crate>=<lvl>"`, `-Tdefmt.x`; on ESP esp-println/esp-backtrace instead |
| rtt | rtt-target, panic-rtt-target | `rtt_init_print!()` at start of main, `rprintln!` |
| esp-println | esp-println `log-04`, log | `ESP_LOG` env, `init_logger_from_env()` |
| none | panic-halt (or esp-backtrace) | |

With crash capture (default on Cortex-M) the panic handler is the one in `src/last_reset.rs` instead of
panic-probe / panic-rtt-target / panic-halt.

## Crash capture (default on Cortex-M, `--no-crash-capture`)

`bedrock_crash` (embedded_bedrock, ROB-11) keeps a 376-byte record in cortex-m-rt's `.uninit` section: cause,
uptime, Cortex-M registers with CFSR / HFSR / MMFAR / BFAR and `EXC_RETURN`, up to 32 stack words, a message (panic
text with location, or the set fault status bits), the build id. The `HardFault` handler in `main.rs` (first
statement `bedrock_crash::exc_return!()`) and the `#[panic_handler]` in `src/last_reset.rs` write it and reset
(`SCB::sys_reset`; a panic with a debugger attached faults instead, so `probe-rs run` stops as with panic-probe).
`last_reset::boot()`, right after logging is set up in `main`, reads the reset cause, checks magic + CRC-32, counts
the boot and logs `last reset: Fault, boot 2` plus the crash. After power-on / brown-out the record is not read
(ECC RAM on H7). The crash stays until `clear()` or a power cycle.

Reset cause: STM32 `RCC_CSR` (`RCC_RSR` on H5 / H7) read as a raw address with the bit numbers from stm32-data, so the
same code serves every framework, cleared with `RMVF`; RP2040 / RP235x `CHIP_RESET` + `WATCHDOG.REASON` through
embassy-rp's PAC; nRF, bare RP and ESP: `Undetermined` (TODO(ROB-11)). Uptime from `embassy_time` with embassy,
otherwise none.

`--wire-weaver` serves the device contract at `device: Device` in the API (TPL-41, `src/ww/device.rs`), with the
items the answers allow: `firmware` (`ww_firmware_info::FirmwareInfo`, served from consts: `firmware_summary!()` from
`ww_firmware_info::build::emit()` in `build.rs`, `BUILD_ID=fn`, `BUILD_RECORD` omitted), `last_reset` (crash
capture), `board`, `uid` (STM32 96-bit id, RP2040 the flash chip's id, RP2350 OTP chip id, nRF FICR DEVICEID),
`label` and `settings` (`--settings`), `counters` (`--counters`), `log` (`--log defmt`, `src/log.rs`), `variant`
(STM32 parts with a `[variant.<dev_id>]` table in `stm32.toml`) and `boot` / `dfu` (`bedrock_rom_boot` on G0 and
single-core H7 without `--bootloader`, needs `--log defmt`; `src/boot.rs`, the boot word in the last backup register,
left out of cnt's BKPSRAM).

`variant.revision` is the API's `SiliconRevision` (TPL-52): `Unknown` (a REV_ID the firmware doesn't know) first, then
the revisions of the errata sheet's "Device variants" table in the sheet's order, `ww_repr = unib32`, with the
catch-all `Newer(u32)` for a revision an older host doesn't know; the raw `rev_id` is beside it. A revision a newer
errata sheet adds goes at the end of the chip's `revisions` in `stm32.toml`, never between, and none is renumbered
or renamed, so taking it into a firmware is an additive API change (the next compatible version, `#[since]` on the
variant). The matrix's `api-evolve` build checks it with `ww api check` (needs `ww` from wire_weaver 668483bd or newer, EVO-10).

Every Cortex-M application links with `--build-id=sha1` (build.rs) and keeps `.note.gnu.build-id` in flash after
`.rodata` (memory.x, symbols `__build_id_start` / `__build_id_end` for `ww_firmware_info::build_id()`); check with
`readelf -n <elf>`. The bootloader doesn't.

## Counters (`cnt` crate)

`cnt!(name: u32|u64 [ "unit" ] [+= expr] [, level [, group]])`, `cnt_if!(cond, <same>)`; `bkp_cnt!` /
`bkp_cnt_if!` for the BKP buffer. The dependency must be named `cnt`. Linker script `-Tcnt.x` (generated by `cnt`'s
build script) allocates indices and fails the link with `cnt: too many RAM/BKP counters` when a buffer is too small.
Env in `.cargo/config.toml`: `CNT_RAM_BUFFER_SIZE_WORDS` [64], `CNT_BKP_BUFFER_SIZE_WORDS` [0],
`CNT_BKP_MEMORY_REGION` [`BKPSRAM`] — the generator names the backup-register region `BKPSRAM` and only sets the env
(to `BKP_REGS`) on chips whose backup SRAM is already called `BKPSRAM`; cnt.x emits the NOLOAD
`.cnt_bkp_buffer` output section into that region, memory.x only declares the region.

Per-instance counters for library/driver crates: `#[derive(cnt::Count)] enum E { A, #[count(u64, unit = "B", warn)] B }`,
driver takes `&'static cnt::Counters<E>` and calls `.count(E::A)`, `.count_if(c, E::A)`, `.add(E::B, n)`; the
firmware declares `static X: cnt::Counters<E> = cnt::counters!(E, x);` (or `bkp_counters!`). Feature
`cnt/disabled` compiles all counters out (cnt.x stays, empty).

Updates use `fetch_add`/`fetch_update` (ldrex/strex) on cores with 32-bit atomics, plain load-add-store on thumbv6m.
Read with `cargo install cnt_cli` → `cnt tui` / `cnt read` / `cnt reset [--bkp]` (chip taken from the probe-rs runner,
newest ELF in the project; ESP needs `--chip`), or `counters_ram_buffer()` (`&[AtomicU32]`) from firmware. Generated
code counts `led_toggles`/`loop_iterations`, `unhandled_exceptions`, and with BKP counters `unhandled_exceptions_total`
and `hard_faults`.

`.cargo/config.toml` (non-ESP) carries a commented `runner = "cnt run --chip <CHIP>"` under the active `probe-rs run`
one: `cnt run` flashes, resets and then shows the defmt log plus live counters as they change, like `cnt read --watch`
but usable as the cargo runner. The generated `justfile`'s `cnt` recipe (`cargo build && cnt run`) uses it without
switching the default runner.

## Build info (`--build-info`)

`bedrock_build::common()` replaces the manual `-Tlink.x/-Tdefmt.x/--nmagic` lines and adds `RAM_LINK=1` support
(link and run from RAM, `link_ram_cortex_m.x`). `bedrock_build::serialize_build_info()` writes `build_info.rs`
with `compact()` (CRC'd blob in flash) and `full()` (defmt-interned, ELF only). Requires a checkout of
`embedded_bedrock` (`--bedrock <relative path>`, default `git`); its WireWeaver crates come from git. With a local
`--ww-src`, `Cargo.toml` and `bootloader/Cargo.toml` get a `[patch."https://github.com/vhrdtech/wire_weaver"]` that
points bedrock_build's `wire_weaver`, `ww_date_time` and `ww_version` (and bedrock_rom_boot's `ww_dfu` with
`device.boot`) at that checkout, so the build has one WireWeaver. The bootloader embeds `compact()` alone unless its `defmt` feature is on (`full()` needs defmt).

## Hubris

Hubris (Oxide) is a different model (kernel + tasks described by `app.toml`, built with `cargo xtask`); it cannot share the
crate skeleton. What can be reused: `hubris-memory --chip X` prints a `memory.toml` (flash / RAM regions from stm32-data)
for `chips/<family>/`. Hubris also needs a `chip.toml` with peripheral addresses/IRQs — the same stm32-data JSON
(`cores[0].peripherals[].address`, `cores[0].interrupts`) has that information; extend `gen_hubris_memory` if needed.
Hubris only supports ARMv6-M/7-M/8-M (STM32F3/F4/G0/H7, LPC55) and needs power-of-two aligned regions for the MPU on v7-M.

## Tested matrix

Every entry generates and `cargo build`s (dev and, where listed, release and the bootloader) with the versions
above. The entries live as data in `../scripts/data/matrix.toml` (name, args, builds, tags — see its header
comment), run by `uv run scripts/bedrock_gen.py matrix` (TPL-44): generates each entry into a scratch directory,
runs the listed `cargo build`s, `rustfmt --edition 2024 --check` on every generated `.rs`, and the flip-link check
below on every Cortex-M ELF; prints a summary table with per-step timings and exits 1 on a failure.

```sh
uv run scripts/bedrock_gen.py matrix                                    # every entry
uv run scripts/bedrock_gen.py matrix g0bi g0bigit                       # only these
uv run scripts/bedrock_gen.py matrix --tag ww                           # only entries tagged ww
uv run scripts/bedrock_gen.py matrix --bedrock <embedded_bedrock checkout> --ww-src <wire_weaver checkout>  # local, not git
```

`--host` entries also pass, from the root: `cargo test --workspace`, `cargo clippy --workspace --all-targets --
-D warnings`, `cargo fmt --all --check` and `just test-py` (maturin build of `<name>_py`, pytest).

Crash capture is on in every Cortex-M entry, built with `--bedrock <embedded_bedrock checkout>`
(`bedrock_crash` is not on crates.io).

flip-link is on in every Cortex-M entry; check it in each ELF: `_stack_start` (`readelf -s`) equals the address of
`.data` (`readelf -S`) and `_stack_end` is the start of `RAM`. Without flip-link `_stack_start` is the end of `RAM`.

WireWeaver entries are built against WireWeaver `master` from git; `--ww-src <checkout>` builds against a local tree.
At least once per release one entry builds with the default git sources (`--bedrock git --ww-src git`, `g0bigit`):
the other entries use sibling checkouts and miss what only a git consumer sees (path dependencies inside
embedded_bedrock, BLD-8; an unreleased WireWeaver change). Before the branch is on GitHub, point the generated
`git = "https://github.com/romixlab/embedded_bedrock"` lines at `file://<checkout>` with `branch = "<branch>"`.
`g0bigit` needs `cargo generate-lockfile` in `g0bigit_api/` first (TPL-43); the `matrix` subcommand does this itself
for any `git`-tagged `--wire-weaver` entry, so it only matters when running `new` by hand.
The USB set-up was also compiled for STM32G474RE (`USB_LP`, needs embassy-stm32 `single-bank`/`dual-bank`, which the
template does not select yet), STM32U585AI (`OTG_FS`, `ICLKSEL`) and STM32H563ZI (`USB_DRD_FS`) after removing the
H7-only `SupplyConfig` lines the SMPS check forces on U5/H5.
Xtensa (esp32/s2/s3) generation is covered, compilation needs the `esp` toolchain and was not run.
