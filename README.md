# Ruggedizing

[ ] flip-link to detect stack overflow - https://github.com/knurling-rs/flip-link
    - If you want to see what flip-link is up to, you can set these environment variables:
        
        > export RUSTC_LOG=rustc_codegen_ssa::back::link=info
        export RUST_LOG=info
        > 
[ ] stack probe to estimate stack usage - cortex-m-rt paint-stack feature
[ ] Periodic FLASH CRC checks
[ ] Enable and monitor RAM ECC
[ ] Periodic periphery config checks
[ ] Use watchdog
[ ] Use window watchdog

# Build infrastructure

[ ] RAM linking option
[ ] Firmware template
[ ] Embed firmware SHA for defmt lookup
[ ] Embed CRC for bootloader
[ ] Bootloader support
[ ] Flash EEPROM emulation (with help from bootloader)
    [ ] Use swap page to safeguard latest config on page erase, restore if power cut in bootloader
    [ ] Restore bootloader state the same way
[ ] Embed build information and provide a way to retrieve it

# Logging

[ ] defmt
[ ] counters
    - embed counters names?
    - more advanced counters (see Hubris debugger)?
[ ] defmt-brtt to use both RTT and ring buffer to retrieve logs
[ ] Log into BKPSRAM and/or save to SD card
[ ] HardFault handler
    - Blink Morse code error (addr + maybe some flags)
    - Optionally reboot after blinking out errors (default) or continue blinking
[ ] UsageFault, MemoryManagement, BusFault handler (to cause less confusion)
[ ] CSS handler

# Testing

[ ] Standard Rust tests with `#![cfg_attr(not(test), no_std)]`
[ ] on MCU tests - https://github.com/probe-rs/embedded-test
[ ] `#[quickcheck]` for random test input into function arguments
[ ] embedded hal mock?

# Usefull tools

- cargo bloat to analyze how big in code size are various functions
- cargo size to check text and data sizes
    - Print binary size in System V format: `cargo size --release -- -A -x`
- cargo-xtask if Rust scripting as a cargo command is needed
- cargo binutils - https://github.com/rust-embedded/cargo-binutils
    - List all symbols in an executable sorted by size (smallest first):
    `cargo nm --release -- --print-size --size-sort`
    - Convert to binary: `cargo objcopy --release -- -O binary app.bin`
    - Disassemble: `cargo objdump --release -- --disassemble --no-show-raw-insn`
[ ] Ozone script
[ ] GDB/LLDB start script
    - gdb dashboard

# Size savings

- Try various optimisations
    
    > opt-level = "z" # 3 - speed, s - size, z - even less size
    > 
- Use defmt
- Build core from sources and/or avoid panic handling - https://doc.rust-lang.org/cargo/reference/unstable.html#build-std
    
    > [unstable]
    build-std = ["core"]
    #build-std-features = ["panic_immediate_abort"] # for even smaller size
    > 

# See also

- Hubris debugger: https://github.com/oxidecomputer/humility?tab=readme-ov-file#commands
