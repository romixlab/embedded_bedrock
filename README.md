# `embedded-bedrock`

Bare metal firmware template, offering ruggedizing features, robust debugging, logging and a helper tool.

## Ruggedizing

* [ ] flip-link to detect stack overflow - https://github.com/knurling-rs/flip-link
    - If you want to see what flip-link is up to, you can set these environment variables:
        
        > export RUSTC_LOG=rustc_codegen_ssa::back::link=info
        export RUST_LOG=info
        > 
* [ ] stack probe to estimate stack usage - cortex-m-rt paint-stack feature
* [ ] Periodic FLASH CRC checks
* [ ] Enable and monitor RAM ECC
* [ ] Periodic periphery config checks
* [ ] Use watchdog
* [ ] Use window watchdog

## Debug tool
* [ ] diagnose?
* [ ] Reset
* [ ] Build, flash and upload binary to local registry (for later defmt decoding based on firmware SHA)
    * Link and run from RAM
* [ ] Halt/Go
* [ ] Show build info from connected target
* [ ] Connect to running target with defmt logging
* [ ] Display event counters
* [ ] Attach with GDB
* [ ] GPIO pin manipulation?
    * [ ] Show current status of all pins
* [ ] bootloader - show state and usefull info
* [ ] Show stack usage
* [ ] Show FLASH and RAM usage
* [ ] Show voltage and core temperature?
* [ ] Read and write memory
* [ ] Show registers
    * [ ] Show where PC is pointing
* [ ] Analyze HardFault
* [ ] Read and display configuration from FLASH
* [ ] ETM support?
* [ ] Observe memory changes?
* [ ] Peripherals manipulations
    * [ ] Show what is enabled and basic configuration
    * [ ] Show clock configuration, calculate PLL frequencies
* [ ] Terminal to the firmware (if fw supports it)

## Build infrastructure

* [ ] RAM linking option
* [ ] Embed firmware SHA for defmt lookup
* [ ] Embed CRC for bootloader
* [ ] Bootloader support
* [ ] Flash EEPROM emulation (with help from bootloader)
    * [ ] Use swap page to safeguard latest config on page erase, restore if power cut in bootloader
    * [ ] Restore bootloader state the same way
* [ ] Embed build information and provide a way to retrieve it
* [ ] git hooks for checking and replacing Cargo path dependencies with git links

## Logging

* [ ] defmt
    * Explicitly set default buffer size and log level
* [ ] counters
    - embed counters names?
    - more advanced counters (see Hubris debugger)?
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

## Usefull tools

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

- Try various optimisations
    
    > opt-level = "z" # 3 - speed, s - size, z - even less size
    > 
- Use defmt
- Build core from sources and/or avoid panic handling - https://doc.rust-lang.org/cargo/reference/unstable.html#build-std
    
    > [unstable]
    build-std = ["core"]
    #build-std-features = ["panic_immediate_abort"] # for even smaller size
    > 

## Other
* [ ] Depend on `portable-atomic = { version = "1.11", features = ["critical-section"] }` to make static_cell work on targets with no atomics (e.g., Thumbv6m)
* [ ] Add TODO items on git dependencies to remember to update them periodically or when crates version is released
* [ ] Ask whether to pin Rust version and whether to use nightly
* [ ] Ask whether to use embassy
* [ ] Ask whether to use RTIC
* [ ] Ask whether to use stm32-xx-hal

## See also

- Hubris debugger: https://github.com/oxidecomputer/humility?tab=readme-ov-file#commands
