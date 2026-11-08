# The tests. `just test`: all of them (test-all), the run of the release gate. `just test --changed`: only the
# workspace crates changed against main and the crates that depend on them, a quick check before work is
# marked ready.
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
    cargo nextest run --locked -p bedrock_led -p bedrock_sync -p bedrock_rom_boot --features bedrock_led/embassy,bedrock_led/ww-indication,bedrock_led/defmt,bedrock_sync/embassy,bedrock_sync/ww-timebase,bedrock_sync/sof,bedrock_sync/defmt,bedrock_rom_boot/ww-dfu,bedrock_rom_boot/defmt
    cargo nextest run --locked -p bedrock_usb --features metapac-check --test v4_metapac
    cargo nextest run --locked -p bedrock_usb --features metapac-check-v3 --test v3_metapac
    cargo nextest run --locked -p bedrock_usb --features metapac-check-v1 --test v1_metapac
    cargo nextest run --locked -p bedrock_usb --features metapac-check-v2 --test v2_metapac
    cargo test --doc --workspace --locked
    cargo test --doc --locked -p bedrock_led -p bedrock_sync -p bedrock_rom_boot --features bedrock_led/embassy,bedrock_led/ww-indication,bedrock_led/defmt,bedrock_sync/embassy,bedrock_sync/ww-timebase,bedrock_sync/sof,bedrock_sync/defmt,bedrock_rom_boot/ww-dfu,bedrock_rom_boot/defmt

# Format check, clippy on the host, and bedrock_led / bedrock_sync / bedrock_can / bedrock_usb as no_std firmware code for the
# Cortex-M0+, bedrock_rom_boot for the G0 (Cortex-M0+) and the H7 (Cortex-M7), bedrock_clocks_rt for the H7
# --locked: a gate recipe must only check, never rewrite Cargo.lock (WireWeaver comes from git at the commit the lock
# names; a lock left in the `just dev-local` state names sibling paths and fails here instead of building against
# whatever the sibling holds)
lint:
    cargo fmt --all --check
    cargo clippy --workspace --all-targets --locked -- -D warnings
    cargo clippy -p bedrock_led --all-targets --all-features --locked -- -D warnings
    cargo clippy -p bedrock_led --target thumbv6m-none-eabi --features embassy,ww-indication,defmt --locked -- -D warnings
    cargo clippy -p bedrock_sync --all-targets --all-features --locked -- -D warnings
    cargo clippy -p bedrock_sync --target thumbv6m-none-eabi --features embassy,ww-timebase,sof,defmt --locked -- -D warnings
    cargo clippy -p bedrock_can --target thumbv6m-none-eabi --features defmt --locked -- -D warnings
    cargo clippy -p bedrock_usb --all-targets --features sim,defmt,metapac-check --locked -- -D warnings
    cargo clippy -p bedrock_usb --all-targets --features sim,defmt,metapac-check-v3 --locked -- -D warnings
    cargo clippy -p bedrock_usb --all-targets --features sim,defmt,metapac-check-v1 --locked -- -D warnings
    cargo clippy -p bedrock_usb --all-targets --features sim,defmt,metapac-check-v2 --locked -- -D warnings
    cargo clippy -p bedrock_usb --target thumbv6m-none-eabi --features defmt --locked -- -D warnings
    cargo clippy -p bedrock_usb --target thumbv6m-none-eabi --features defmt,embassy-stm32,embassy-stm32/stm32g0b1re --locked -- -D warnings
    cargo clippy -p bedrock_usb --target thumbv7em-none-eabihf --features 'defmt,embassy-stm32-otg,embassy-stm32?/stm32h725ig' --locked -- -D warnings
    cargo clippy -p bedrock_usb --target thumbv7em-none-eabihf --features defmt,embassy-stm32,embassy-stm32/stm32g474re,embassy-stm32/single-bank --locked -- -D warnings
    cargo clippy -p bedrock_usb --target thumbv7em-none-eabihf --features defmt,embassy-stm32,embassy-stm32/stm32l432kc --locked -- -D warnings
    cargo clippy -p bedrock_usb --target thumbv7em-none-eabihf --features defmt,embassy-stm32,embassy-stm32/stm32wb55rg --locked -- -D warnings
    cargo clippy -p bedrock_usb --target thumbv6m-none-eabi --features defmt,embassy-stm32,embassy-stm32/stm32f072rb --locked -- -D warnings
    cargo clippy -p bedrock_usb --target thumbv6m-none-eabi --features defmt,embassy-stm32,embassy-stm32/stm32l053r8 --locked -- -D warnings
    cargo clippy -p bedrock_usb --target thumbv7m-none-eabi --features defmt,stm32f1,embassy-stm32/stm32f103c8 --locked -- -D warnings
    cargo clippy -p bedrock_usb --target thumbv7em-none-eabihf --features defmt,embassy-stm32,embassy-stm32/stm32f303vc --locked -- -D warnings
    cargo clippy -p bedrock_usb --target thumbv7em-none-eabihf --features defmt,embassy-stm32,embassy-stm32/stm32f303re --locked -- -D warnings
    cargo clippy -p bedrock_usb --target thumbv7em-none-eabihf --features defmt,embassy-stm32,embassy-stm32/stm32f373cc --locked -- -D warnings
    # embassy-stm32 0.6's L1 ADC driver uses embassy-time without asking for it: its `time` feature turns it on
    cargo clippy -p bedrock_usb --target thumbv7m-none-eabi --features defmt,embassy-stm32,embassy-stm32/stm32l151cb,embassy-stm32/time --locked -- -D warnings
    cargo fmt --manifest-path {{usb_example}}/Cargo.toml --check
    cargo clippy --manifest-path {{usb_example}}/Cargo.toml --target thumbv6m-none-eabi --release --locked -- -D warnings
    cargo build --manifest-path {{usb_example}}/Cargo.toml --target thumbv6m-none-eabi --release --locked
    CARGO_PROFILE_RELEASE_OPT_LEVEL=3 cargo build --manifest-path {{usb_example}}/Cargo.toml --target thumbv6m-none-eabi --release --locked --target-dir {{usb_example}}/target/o3
    cargo clippy --manifest-path {{usb_example}}/Cargo.toml --target thumbv6m-none-eabi --release --features sof --locked --target-dir {{usb_example}}/target/sof -- -D warnings
    cargo build --manifest-path {{usb_example}}/Cargo.toml --target thumbv6m-none-eabi --release --features sof --locked --target-dir {{usb_example}}/target/sof
    cargo fmt --manifest-path {{usb_example_h7}}/Cargo.toml --check
    cargo clippy --manifest-path {{usb_example_h7}}/Cargo.toml --target thumbv7em-none-eabihf --release --locked -- -D warnings
    cargo build --manifest-path {{usb_example_h7}}/Cargo.toml --target thumbv7em-none-eabihf --release --locked
    CARGO_PROFILE_RELEASE_OPT_LEVEL=3 cargo build --manifest-path {{usb_example_h7}}/Cargo.toml --target thumbv7em-none-eabihf --release --locked --target-dir {{usb_example_h7}}/target/o3
    cargo fmt --manifest-path {{usb_example_v3}}/Cargo.toml --check
    cargo clippy --manifest-path {{usb_example_v3}}/Cargo.toml --target thumbv7em-none-eabihf --release --locked -- -D warnings
    cargo build --manifest-path {{usb_example_v3}}/Cargo.toml --target thumbv7em-none-eabihf --release --locked
    CARGO_PROFILE_RELEASE_OPT_LEVEL=3 cargo build --manifest-path {{usb_example_v3}}/Cargo.toml --target thumbv7em-none-eabihf --release --locked --target-dir {{usb_example_v3}}/target/o3
    cargo fmt --manifest-path {{usb_example_v1}}/Cargo.toml --check
    cargo clippy --manifest-path {{usb_example_v1}}/Cargo.toml --target thumbv7m-none-eabi --release --locked -- -D warnings
    cargo build --manifest-path {{usb_example_v1}}/Cargo.toml --target thumbv7m-none-eabi --release --locked
    CARGO_PROFILE_RELEASE_OPT_LEVEL=3 cargo build --manifest-path {{usb_example_v1}}/Cargo.toml --target thumbv7m-none-eabi --release --locked --target-dir {{usb_example_v1}}/target/o3
    cargo clippy -p bedrock_rom_boot --all-targets --features ww-dfu,defmt --locked -- -D warnings
    cargo clippy -p bedrock_rom_boot --target thumbv6m-none-eabi --features stm32g0,embassy,ww-dfu,defmt --locked -- -D warnings
    cargo clippy -p bedrock_rom_boot --target thumbv7em-none-eabihf --features stm32h7,embassy,ww-dfu,defmt --locked -- -D warnings
    cargo clippy -p bedrock_clocks_rt --target thumbv7em-none-eabihf --features defmt --locked -- -D warnings

# bedrock_usb's example firmware (STM32G0B1, vendor bulk loopback): a standalone crate with its own Cargo.lock, built
# by `lint` at opt-level z (its release profile) and 3, and with SOF timestamps (feature `sof`) at z
usb_example := "bedrock_usb/examples/g0b1_loopback"
# the same on the OTG driver: STM32H725, OTG_HS over ULPI, high speed
usb_example_h7 := "bedrock_usb/examples/h725_loopback"
# the same device on usb_v3 (STM32G474, 16-bit USB SRAM)
usb_example_v3 := "bedrock_usb/examples/g474_loopback"
# and on usb_v1 (STM32F103, 16-bit USB SRAM spaced one half-word per word)
usb_example_v1 := "bedrock_usb/examples/f103_loopback"

# Code bytes of the examples per crate and per bedrock_usb file (DWARF line table), at opt-level z and 3 and the
# G0B1 with `sof` at z, after `lint`
usb-size:
    cargo run -q --locked -p bedrock_lint_cli -- code-size {{usb_example}}/target/thumbv6m-none-eabi/release/g0b1_loopback --by-crate --top 8
    cargo run -q --locked -p bedrock_lint_cli -- code-size {{usb_example}}/target/thumbv6m-none-eabi/release/g0b1_loopback -m bedrock_usb/src
    cargo run -q --locked -p bedrock_lint_cli -- code-size {{usb_example}}/target/o3/thumbv6m-none-eabi/release/g0b1_loopback --by-crate --top 8
    cargo run -q --locked -p bedrock_lint_cli -- code-size {{usb_example}}/target/o3/thumbv6m-none-eabi/release/g0b1_loopback -m bedrock_usb/src
    cargo run -q --locked -p bedrock_lint_cli -- code-size {{usb_example}}/target/sof/thumbv6m-none-eabi/release/g0b1_loopback --by-crate --top 8
    cargo run -q --locked -p bedrock_lint_cli -- code-size {{usb_example_h7}}/target/thumbv7em-none-eabihf/release/h725_loopback --by-crate --top 8
    cargo run -q --locked -p bedrock_lint_cli -- code-size {{usb_example_h7}}/target/thumbv7em-none-eabihf/release/h725_loopback -m bedrock_usb/src
    cargo run -q --locked -p bedrock_lint_cli -- code-size {{usb_example_h7}}/target/o3/thumbv7em-none-eabihf/release/h725_loopback --by-crate --top 8
    cargo run -q --locked -p bedrock_lint_cli -- code-size {{usb_example_h7}}/target/o3/thumbv7em-none-eabihf/release/h725_loopback -m bedrock_usb/src
    cargo run -q --locked -p bedrock_lint_cli -- code-size {{usb_example_v3}}/target/thumbv7em-none-eabihf/release/g474_loopback --by-crate --top 8
    cargo run -q --locked -p bedrock_lint_cli -- code-size {{usb_example_v3}}/target/thumbv7em-none-eabihf/release/g474_loopback -m bedrock_usb/src
    cargo run -q --locked -p bedrock_lint_cli -- code-size {{usb_example_v3}}/target/o3/thumbv7em-none-eabihf/release/g474_loopback --by-crate --top 8
    cargo run -q --locked -p bedrock_lint_cli -- code-size {{usb_example_v3}}/target/o3/thumbv7em-none-eabihf/release/g474_loopback -m bedrock_usb/src
    cargo run -q --locked -p bedrock_lint_cli -- code-size {{usb_example_v1}}/target/thumbv7m-none-eabi/release/f103_loopback --by-crate --top 8
    cargo run -q --locked -p bedrock_lint_cli -- code-size {{usb_example_v1}}/target/thumbv7m-none-eabi/release/f103_loopback -m bedrock_usb/src
    cargo run -q --locked -p bedrock_lint_cli -- code-size {{usb_example_v1}}/target/o3/thumbv7m-none-eabi/release/f103_loopback --by-crate --top 8
    cargo run -q --locked -p bedrock_lint_cli -- code-size {{usb_example_v1}}/target/o3/thumbv7m-none-eabi/release/f103_loopback -m bedrock_usb/src

# Build bedrock-lint (the firmware lint) and install it on this PC
install:
    cargo build --profile deploy --locked -p bedrock_lint_cli
    # With an optional internal release-management tool (tpm) on PATH: install through it (kept-releases stack,
    # one copy on PATH); otherwise: cargo install
    if command -v tpm >/dev/null; then tpm install target/deploy/bedrock-lint; else cargo install --locked --path bedrock_lint_cli; fi

# Build against the sibling WireWeaver checkout ../wire_weaver (co-development): .cargo/sibling-patch.toml becomes
# the gitignored .cargo/dev-local.toml. Cargo.lock then names the sibling paths: don't commit it in this state.
dev-local:
    cp .cargo/sibling-patch.toml .cargo/dev-local.toml
    cargo tree --workspace -i wire_weaver --target all --depth 0

# Back to WireWeaver from git, the committed setup. Cargo relocks to wire_weaver's main if the lock lost its git
# pin under dev-local: `git diff Cargo.lock` shows whether the pin moved (commit it as a bump, or check out the lock).
dev-git:
    rm -f .cargo/dev-local.toml
    cargo tree --workspace -i wire_weaver --target all --depth 0
    git diff --stat Cargo.lock
