# The tests. `just test`: all of them (test-all), the run of the `tpm land` gate. `just test --changed`: only the
# workspace crates changed against main and the crates that depend on them (tpm test changed), what a session runs
# before tpm work ready.
test *flags:
    if [ "{{flags}}" = "--changed" ]; then tpm test changed -- cargo nextest run --workspace; else {{just_executable()}} test-all {{flags}}; fi

# Host tests of the workspace crates, bedrock_led and bedrock_sync with every feature
test-all:
    cargo test --workspace
    cargo test -p bedrock_led --all-features
    cargo test -p bedrock_sync --all-features

# Format check, clippy on the host, and bedrock_led / bedrock_sync as no_std firmware code for the Cortex-M0+
lint:
    cargo fmt --all --check
    cargo clippy --workspace --all-targets -- -D warnings
    cargo clippy -p bedrock_led --all-targets --all-features -- -D warnings
    cargo clippy -p bedrock_led --target thumbv6m-none-eabi --features embassy,ww-indication,defmt -- -D warnings
    cargo clippy -p bedrock_sync --all-targets --all-features -- -D warnings
    cargo clippy -p bedrock_sync --target thumbv6m-none-eabi --features embassy,ww-timebase,defmt -- -D warnings
