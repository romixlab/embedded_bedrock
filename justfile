# The tests. `just test`: all of them (test-all), the run of the `tpm land` gate. `just test --changed`: only the
# workspace crates changed against main and the crates that depend on them (tpm test changed), what a session runs
# before tpm work ready.
test *flags:
    if [ "{{flags}}" = "--changed" ]; then tpm test changed -- cargo nextest run --workspace; else {{just_executable()}} test-all {{flags}}; fi

# Host tests of the workspace crates, bedrock_led and bedrock_sync with every feature, bedrock_rom_boot with its
# host features (a chip family is firmware only). Two nextest runs, not four cargo test runs: the first is every
# crate at its default (no extra) features, which is the only run that still covers bedrock_led / bedrock_sync /
# bedrock_rom_boot's HAL-free, feature-off build (nextest can't tell "no features" from "fewer features", so this
# run must stay separate, never folded into the second); the second unions the three crates' extra feature passes
# into one build instead of three, so wire_weaver, embassy-time and friends compile once for all of them instead
# of once per crate. `pkg/feature` scopes each feature to its crate: a bare `--features embassy` would also turn
# embassy on for bedrock_rom_boot, which needs a chip family (firmware only) for that to make sense.
# Doctests separately: nextest doesn't run them (its own limitation), and they're cheap next to the unit tests'
# linking cost.
test-all:
    cargo nextest run --workspace --locked
    cargo nextest run --locked -p bedrock_led -p bedrock_sync -p bedrock_rom_boot --features bedrock_led/embassy,bedrock_led/ww-indication,bedrock_led/defmt,bedrock_sync/embassy,bedrock_sync/ww-timebase,bedrock_sync/defmt,bedrock_rom_boot/ww-dfu,bedrock_rom_boot/defmt
    cargo test --doc --workspace --locked
    cargo test --doc --locked -p bedrock_led -p bedrock_sync -p bedrock_rom_boot --features bedrock_led/embassy,bedrock_led/ww-indication,bedrock_led/defmt,bedrock_sync/embassy,bedrock_sync/ww-timebase,bedrock_sync/defmt,bedrock_rom_boot/ww-dfu,bedrock_rom_boot/defmt

# Format check, clippy on the host, and bedrock_led / bedrock_sync / bedrock_can as no_std firmware code for the
# Cortex-M0+, bedrock_rom_boot for the G0 (Cortex-M0+) and the H7 (Cortex-M7), bedrock_clocks_rt for the H7
# --locked: a gate recipe must only check, never rewrite Cargo.lock (the `../wire_weaver` path patch in
# .cargo/config.toml makes the lock sensitive to whatever commit that sibling is at; without --locked a moved
# sibling makes cargo silently relock instead of erroring)
lint:
    cargo fmt --all --check
    cargo clippy --workspace --all-targets --locked -- -D warnings
    cargo clippy -p bedrock_led --all-targets --all-features --locked -- -D warnings
    cargo clippy -p bedrock_led --target thumbv6m-none-eabi --features embassy,ww-indication,defmt --locked -- -D warnings
    cargo clippy -p bedrock_sync --all-targets --all-features --locked -- -D warnings
    cargo clippy -p bedrock_sync --target thumbv6m-none-eabi --features embassy,ww-timebase,defmt --locked -- -D warnings
    cargo clippy -p bedrock_can --target thumbv6m-none-eabi --features defmt --locked -- -D warnings
    cargo clippy -p bedrock_rom_boot --all-targets --features ww-dfu,defmt --locked -- -D warnings
    cargo clippy -p bedrock_rom_boot --target thumbv6m-none-eabi --features stm32g0,embassy,ww-dfu,defmt --locked -- -D warnings
    cargo clippy -p bedrock_rom_boot --target thumbv7em-none-eabihf --features stm32h7,embassy,ww-dfu,defmt --locked -- -D warnings
    cargo clippy -p bedrock_clocks_rt --target thumbv7em-none-eabihf --features defmt --locked -- -D warnings

# Build bedrock-lint (the firmware lint) and install it on this PC
install:
    cargo build --profile deploy --locked -p bedrock_lint_cli
    # With tpm on PATH: tpm install (kept-releases stack, one copy on PATH); otherwise: cargo install
    if command -v tpm >/dev/null; then tpm install target/deploy/bedrock-lint; else cargo install --locked --path bedrock_lint_cli; fi
