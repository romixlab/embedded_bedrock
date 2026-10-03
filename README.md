# `embedded-bedrock`

Bare metal firmware template, offering ruggedizing features, robust debugging, logging and a helper tool.
Optional embassy, RTIC and hal crates support.

## Prerequisites

* [probe-rs](https://probe.rs/docs/getting-started/installation/)
* `cargo install flip-link`

## Features, bugs and plans

What works and what is planned (firmware template, device contract, build info, robustness, debug tool) is in
[FEATURES.md](FEATURES.md). Changes: [firmware_template_skill/CHANGELOG.md](firmware_template_skill/CHANGELOG.md)
for the template, [CHANGELOG.md](CHANGELOG.md) for the crates. Rules for contributors and agents:
[AGENTS.md](AGENTS.md).

The firmware template is the `firmware-template` agent skill in [firmware_template_skill/](firmware_template_skill/SKILL.md).

### Template disclaimer

Due to the sheer number of microcontrollers it is next to impossible to cover all edge cases, though for most
STM32s and a few hand-coded MCUs from other vendors most functionality should work. Generate a test project (for
example for STM32H725IG) to evaluate the template before relying on it.

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
