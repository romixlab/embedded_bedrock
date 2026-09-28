# Frameworks, families, versions

## Crate versions (verified on crates.io, see `V` in the script)

embassy-executor 0.10 · embassy-time 0.5.1 · embassy-sync 0.8 · embassy-stm32 0.6 · embassy-rp 0.10 · embassy-nrf 0.11 ·
embassy-boot-{stm32 0.8, rp 0.10, nrf 0.12} (all on embassy-boot 0.7) · esp-hal 1.2 · esp-rtos 0.4 · esp-println 0.18 ·
esp-backtrace 0.20 · esp-bootloader-esp-idf 0.6 · stm32-hal2 2.1 · cnt 0.2 · defmt 1.1 · defmt-rtt 1.3 · panic-probe 1.0 ·
rtt-target 0.6 · cortex-m 0.7 · cortex-m-rt 0.7 · build-info-build 0.0.46.

Pitfalls with older documentation / examples:
- `embassy-executor` `task-arena-size-*` and `nightly` features: gone since 0.10, tasks are statically allocated on
  stable → no nightly needed, no `#![feature(impl_trait_in_assoc_type)]`.
- `cnt 0.2` requires `cnt_if!(cond, name: u32 += 1)`; `cnt_macro` is re-exported by `cnt`.
- `embassy-stm32`: prefer `time-driver-any` over `time-driver-timN`. `SMPSSupplyVoltage::V1_8`/`V2_5`.
- `bedrock_build` build-info is optional (`--build-info`) since `bedrock_build_info` depends on an unpublished `ww_date_time`.

## Per family

### STM32 (embassy)
`embassy-stm32` features: `<chip>`, `unstable-pac`, `exti`, `time`, `time-driver-any`, `defmt`, `chrono` with `--rtc`.
`cortex-m` `critical-section-single-core`. thumbv6m adds `portable-atomic` with `critical-section`.
Bootloader: `Flash::new_blocking(p.FLASH).into_blocking_regions().bank1_region`, `BootLoader::prepare::<_,_,_,ERASE>`
with the largest erase size, `bl.load(BANK1_REGION.base() + active_offset)`.

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
probe-rs chips: `RP2040`, `RP235x`. Alternative runners: `picotool load -u -v -x -t elf`, `elf2uf2-rs -d`.

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

### stm32-hal2 (`--framework stm32-hal`)
See `stm32-notes.md`. Blocking `#[entry]` skeleton, `hal::pac`, no bootloader integration (embassy-boot still usable via
`embedded-storage` traits, code must be added manually).

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

## Counters (`cnt` crate)

`cnt_if!(cond, name: u32|u64 [ "unit" ] += expr [, level [, group]])`, `bkp_cnt_if!` for backup registers. Linker script
`-Tcnt.x` allocates indices; buffer sizes via `CNT_RAM_BUFFER_SIZE_WORDS` / `CNT_BKP_BUFFER_SIZE_WORDS` env in
`.cargo/config.toml`. Read with `cnt_cli <elf> tui` or `counters_ram_buffer()` from firmware. Generated code counts
`led_toggles`, `unhandled_exceptions`, and with backup counters `hard_faults`.

## Build info (`--build-info`)

`bedrock_build::common()` replaces the manual `-Tlink.x/-Tdefmt.x/--nmagic` lines and adds `RAM_LINK=1` support
(link and run from RAM, `link_ram_cortex_m.x`). `bedrock_build::serialize_build_info()` writes `build_info.rs`
with `compact()` (CRC'd blob in flash) and `full()` (defmt-interned, ELF only). Requires a checkout of
`embedded_bedrock` whose `bedrock_build_info` dependencies resolve (`--bedrock <relative path>`, default `git`).

## Hubris

Hubris (Oxide) is a different model (kernel + tasks described by `app.toml`, built with `cargo xtask`); it cannot share the
crate skeleton. What can be reused: `hubris-memory --chip X` prints a `memory.toml` (flash / RAM regions from stm32-data)
for `chips/<family>/`. Hubris also needs a `chip.toml` with peripheral addresses/IRQs — the same stm32-data JSON
(`cores[0].peripherals[].address`, `cores[0].interrupts`) has that information; extend `gen_hubris_memory` if needed.
Hubris only supports ARMv6-M/7-M/8-M (STM32F3/F4/G0/H7, LPC55) and needs power-of-two aligned regions for the MPU on v7-M.

## Tested matrix

Every entry below generates and `cargo build`s (dev and, where listed, release) with the versions above:

```
new h7app  --chip STM32H725IG --bootloader --config-page --counters --bkp-counters rtc --supply-config DirectSMPS  (+bootloader, release)
new g0     --chip STM32G0B1RE --bootloader --config-page --counters --bkp-counters tamp                            (+bootloader, release)
new g0hal  --chip STM32G0B1RE --framework stm32-hal --config-page --counters
new h7hal  --chip STM32H725IG --framework stm32-hal --supply-config DirectSMPS --counters
new f4bare --chip STM32F407VG --framework bare --log rtt
new l4     --chip STM32L476RG --counters --log none
new l4rtc  --chip STM32L476RG --rtc
new rp2040 --chip rp2040 --bootloader --config-page --counters                                                     (+bootloader, release)
new rp2350 --chip rp2350 --bootloader                                                                              (+bootloader)
new nrf    --chip nrf52840 --bootloader --counters                                                                 (+bootloader)
new nrf9160 --chip nrf9160 --counters --log rtt
new espc3  --chip esp32c3 --counters            (embassy, defmt via esp-println)
new espc3p --chip esp32c3 --log esp-println
new espc3b --chip esp32c3 --framework bare --log esp-println
```
Xtensa (esp32/s2/s3) generation is covered, compilation needs the `esp` toolchain and was not run.
