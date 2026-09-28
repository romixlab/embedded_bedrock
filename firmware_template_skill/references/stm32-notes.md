# STM32 specifics

## Data source

`https://raw.githubusercontent.com/embassy-rs/stm32-data-generated/main/data/chips/<PART>.json`
(≈1000 parts, name without package/temperature suffix, e.g. `STM32H725IG`, `STM32G0B1RE`, `STM32L100C6-A`).
Cached in `~/.cache/bedrock_gen/chips/`. Structure used by the script:

```
name, family ("STM32H7"), line ("STM32H725/735"), die, device_id
memory[0][]        : {name, kind: flash|ram|eeprom|otp, address, size, settings{erase_size, write_size}}
                     flash names: BANK_1, BANK_2, BANK_1_REGION_1.. (F4, differing sector sizes), OTP (ignored)
cores[0].peripherals[] : {name, address, registers{kind, version, block}, rcc{enable{register,field}, ...}, pins[]}
packages[]         : {name, package, pins[{position, signals[]}]}
docs[]             : {type, title, name, url}
```

Register descriptions: `data/registers/<kind>_<version>.json` with `block/<BLOCK>.items[]{name, byte_offset,
fieldset, array{len,stride}}` and `fieldset/<NAME>.fields[]{name, bit_offset, bit_size}`. The script uses these
to find: PWR register containing `DBP`, RCC register containing `BDRST`, RCC `*ENR` fields `<SRAM>EN`,
`BKPR` arrays in TAMP/RTC.

`chip-info --json` dumps the raw chip JSON.

## Rust targets by series

C0/F0/G0/L0/U0 → `thumbv6m-none-eabi`; F1/F2/L1 → `thumbv7m-none-eabi`; F3/F4/F7/G4/H7/L4/WL/WB → `thumbv7em-none-eabihf`;
H5/L5/U5/U3/N6/WBA → `thumbv8m.main-none-eabihf`. (F4/G4/L4 have an FPU, use hard-float.)
Override with `--rust-target`.

## SMPS / SupplyConfig (H7 dual-core & H72x/H73x/H7Ax/H7Bx/H7Rx/H7Sx)

If any pin carries an `SMPS*` signal, the part has an internal step-down and `embassy_stm32::Config::rcc.supply_config`
**must** match the board wiring, otherwise the MCU may not start or may be damaged:

| variant | board wiring |
|---|---|
| `Default` | leave POR setting (LDO), safe on Nucleo boards without SMPS |
| `LDO` | VCORE from internal LDO, SMPS unused |
| `DirectSMPS` | SMPS output feeds VCORE directly |
| `SMPSLDO(V1_8\|V2_5)` | SMPS feeds LDO which feeds VCORE |
| `SMPSExternalLDO(V)` | SMPS feeds an external LDO and VCORE via LDO |
| `SMPSExternalLDOBypass(V)` | SMPS feeds external supply, internal LDO bypassed |
| `SMPSDisabledLDOBypass` | external VCORE supply |

Ask the user for the schematic; the script refuses to generate without `--supply-config`.
The same value is applied to the bootloader crate.

## Backup domain

When the RTC is not used (`--rtc` absent) `src/init.rs::reset_backup_domain()` pulses `RCC.BDCR.BDRST` at boot:
backup-domain contents can be corrupted after a brown-out without a POR, leading to LSE starting itself, PC13–15
changing mode etc. (efton.sk gotchas g133/g62). Register names come from stm32-data (PWR enable register/field,
`DBP` register, `BDCR`).

With `--bkp-counters` the domain is **not** reset; write access is enabled instead. H7 backup registers are
`RTC.BKPnR` at `RTC + 0x50`, G0's are `TAMP.BKPnR` at `TAMP + 0x100`; the script reads the offsets from the register JSON. BKPSRAM (H7 4K backup SRAM) is not
handled automatically.

## stm32-hal2 (`--framework stm32-hal`)

Feature = 4-char device key (`g0b1`, `h735` for H723/725/730/733/735, `l4x6` for L476/486/496, ...) plus the runtime
feature `<series>rt`. Table `STM32_HAL2_FEATURES` in the script; not every part is supported (no F0/F1/F2/F7/L0/L1/U5).
Generated main is blocking (`#[entry]`, `Clocks::default().setup()`, `Pin::new(Port::B, 14, PinMode::Output)`).
`init.rs`/backup-domain code is only generated for embassy (uses `embassy_stm32::pac`); with stm32-hal2 use
`hal::pac` and port it by hand if needed. `init_ram.rs` is generated with TODO comments for the enable bits.

## stm32-rs HALs (`--framework stm32xx-hal`)

Crate and features from `[xxhal.<series>]` in `data/stm32.toml`, see `frameworks.md`. `chip-info` prints the selection.
Like stm32-hal2: `init.rs` is embassy-only, `init_ram.rs` gets TODO comments for the SRAM enable bits.

## H7 caches

`main.rs` enables I-cache; D-cache is left commented because DMA buffers in cached memory need explicit
clean/invalidate. AXISRAM is used as main RAM; DTCM (0x20000000) is not DMA-accessible from most masters on H7.

## Dual bank flash

Both banks are merged into one linear `FLASH` for linking. embassy-stm32's `Flash::into_blocking_regions()`
exposes `bank1_region`/`bank2_region`; the generated bootloader uses `bank1_region` only — a DFU region in bank 2
needs the `bank2_region` (or the whole `Flash`) passed to `BootLoaderConfig` (TODO in the generated code).
