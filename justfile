# The tests. `just test`: all of them (test-all), the run of the `tpm land` gate. `just test --changed`: only the
# workspace crates changed against main and the crates that depend on them (tpm test changed), what a session runs
# before tpm work ready.
test *flags:
    if [ "{{flags}}" = "--changed" ]; then tpm test changed -- cargo nextest run --workspace; else {{just_executable()}} test-all {{flags}}; fi

# Host tests of the workspace crates, bedrock_led and bedrock_sync with every feature, bedrock_rom_boot with its
# host features (a chip family is firmware only)
test-all:
    cargo test --workspace --locked
    cargo test -p bedrock_led --all-features --locked
    cargo test -p bedrock_sync --all-features --locked
    cargo test -p bedrock_rom_boot --features ww-dfu,defmt --locked

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
