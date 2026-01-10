# `embedded-bedrock`

Bare metal firmware template, offering ruggedizing features, robust debugging, logging and a helper tool.
Optional embassy, RTIC and hal crates support.

## Prerequisites

* [probe-rs](https://probe.rs/docs/getting-started/installation/)
* `cargo install flip-link`

## Debug tool

* [ ] Build, flash and upload binary to local registry (for later defmt decoding based on firmware SHA)
    * Link and run from RAM
* [ ] Diagnose target state and common pitfalls
* [ ] Reset
* [ ] Halt/Go
* [ ] Show build info from connected target
* [ ] Connect to running target with defmt logging, optionally fetching binary from registry
* [ ] Display event counters
* [ ] Attach with GDB
* [ ] GPIO pin manipulation
    * [ ] Show current status of all pins
    * [ ] Reconfigure/Set/Reset pins
* [ ] bootloader - show state and useful info
* [ ] Show stack usage
* [ ] Show watchdog information
* [ ] Show RAM and FLASH ECC info
* [ ] Check FLASH CRC
* [ ] Show FLASH and RAM usage
* [ ] Show voltage and core temperature?
* [ ] Read and write memory
* [ ] Show registers with SVD decoding
    * [ ] Show where PC is pointing
* [ ] Analyze HardFault
* [ ] Read and display configuration from FLASH
* [ ] ETM support?
* [ ] Observe memory changes at address (raw or at variable name)
* [ ] Peripherals manipulations
    * [ ] Show what is enabled and basic configuration
    * [ ] Show clock configuration, calculate PLL frequencies
* [ ] Show interrupt information (enabled, priorities, default handler or not, is in RAM)
* [ ] Terminal to the firmware (if fw supports it)
* [ ] Install tools (flip-link, probe-rs, binutils, etc)
* [ ] embassy debug? CPU load

## Build infrastructure

* [ ] Embed CRC for bootloader
* [ ] Save build into to file and inject into ELF (no source modifications)
* [ ] Embed FLASH SHA for quick comparisons and defmt lookup
* [ ] RAM linking option
* [ ] Bootloader support
* [ ] Flash EEPROM emulation (with help from bootloader)
    * [ ] Use swap page to safeguard latest config on page erase, restore if power cut in bootloader
    * [ ] Restore bootloader state the same way
* [ ] Embed build information and provide a way to retrieve it
* [ ] git hooks for checking and replacing Cargo path dependencies with git links

## Ruggedizing

* [ ] flip-link to detect stack overflow - https://github.com/knurling-rs/flip-link
    - If you want to see what flip-link is up to, you can set these environment variables:

      > export RUSTC_LOG=rustc_codegen_ssa::back::link=info
      export RUST_LOG=info

>

* [ ] stack probe to estimate stack usage - cortex-m-rt paint-stack feature

## Firmware template

### Disclaimer

Note that due to the sheer amount of microcontrollers out there it is next to impossible to cover all the edge cases, though
for most of the STM32's and a few hand-coded MCUs from other vendors majority of functionality should work.
Please generate a test project for example for STM32H725IG to evaluate the full capabilities of this template and decide whether you want to use or modify it for your needs.

* [x] Load STM32 data from stm32-data-generated to generate memory.x and others
    * [x] Generate memory.x
    * [x] Check if package contains SMPS pins and ask for a correct configuration option
    * [x] Generate README.md with links to all provided datasheets
* [x] Ask whether RTC is used, and if it is not - reset RTC according to the quirk fix
* [x] Generate rust-toolchain.toml file fixing the currently installed nightly version (if the nightly option is chosen)
* [x] Enable more elaborate size optimizations (build_core, panic_immediate_abort)
* [x] Configure defmt buffer size and disable blocking option if chosen to do so
* [x] Setup counters and configure buffer sizes
* [x] Generate SRAM blocks enable and zero code
* [x] Generate bootloader project code and linker sections (if enabled) TODO: do not generate if disabled
* [x] Reserve 1 FLASH page for permanent config and create a linker section (if enabled)
* [ ] Relocate vector table to SRAM
* [x] Support dual bank FLASH devices with bootloader enabled - place DFU region into the second bank. 

### Other

* [x] Depend on `portable-atomic = { version = "1.11", features = ["critical-section"] }` to make static_cell work on
  targets with no atomics (e.g., Thumbv6m)
* [ ] Add TODO items on git dependencies to remember to update them periodically or when crates version is released
* [x] Ask whether to pin Rust version and whether to use nightly
* [x] Ask whether to use embassy
* [ ] Ask whether to use RTIC
* [ ] Ask whether to use stm32-xx-hal
* [ ] Add TODO item if defmt buffer is small

## Logging

* [x] defmt
    * Explicitly set default buffer size and log level
* [x] counters
    - embed counters names?
    - more advanced counters (see Hubris debugger)?
    - use same mechanism for tracing (store time differences instead of counts)?
* [ ] defmt-brtt to use both RTT and ring buffer to retrieve logs
* [ ] Log into BKPSRAM and/or save to SD card
* [ ] HardFault handler
    - Blink Morse code error (addr + maybe some flags)
    - Optionally reboot after blinking out errors (default) or continue blinking
* [ ] UsageFault, MemoryManagement, BusFault handler (to cause less confusion)
* [ ] CSS handler

## Testing

* [ ] Standard Rust tests with `#![cfg_attr(not(test), no_std)]`
* [ ] on MCU tests - https://github.com/probe-rs/embedded-test
* [ ] `#[quickcheck]` for random test input into function arguments
* [ ] embedded hal mock?

## Useful tools

- cargo bloat to analyze how big in code size are various functions
- cargo size to check text and data sizes
    - Print binary size in System V format: `cargo size --release -- -A -x`
- cargo-xtask if Rust scripting as a cargo command is needed
- cargo binutils - https://github.com/rust-embedded/cargo-binutils
    - List all symbols in an executable sorted by size (smallest first):
      `cargo nm --release -- --print-size --size-sort`
    - Convert to binary: `cargo objcopy --release -- -O binary app.bin`
    - Disassemble: `cargo objdump --release -- --disassemble --no-show-raw-insn`

* [ ] Ozone script
* [ ] GDB/LLDB start script
    - gdb dashboard

## Size savings

- Try various optimizations

  > opt-level = "z" # 3 - speed, s - size, z - even less size

>

- Use defmt
- Build core from sources and/or avoid panic
  handling - https://doc.rust-lang.org/cargo/reference/unstable.html#build-std

  > [unstable]
  build-std = ["core"]
  #build-std-features = ["panic_immediate_abort"] # for even smaller size

>

## See also

- Inspired by: [Hubris debugger](https://github.com/oxidecomputer/humility?tab=readme-ov-file#commands)
- Borrowed some code from: [embassy-template](https://github.com/lulf/embassy-template/tree/main)
- Borrowed some code from: [app-template](https://github.com/knurling-rs/app-template)
