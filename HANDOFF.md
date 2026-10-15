# Handoff: bedrock-crash-capture (P2401#15oct-1115b, ROB-11)

Session branch `s/15oct2026-bedrock-crash-capture`. Design: tpm `prj/P2401-wireweaver/design/device_contract.md`
(LastReset, build id, Risks).

## Done

- `bedrock_crash` crate (no_std, no WireWeaver dependency): `Record` in `.uninit` with magic + CRC-32, `boot()`,
  `capture_fault()` / `capture_panic()`, `reset()` / `panic_reset()` / `reset_with(Updated)`, `clear()`, `Snapshot`,
  STM32 and RP reset-flag decoding. 22 host tests (`cargo test -p bedrock_crash`); builds and clippy-clean for
  thumbv6m, thumbv7em, thumbv8m.main with `--features cortex-m,defmt`.
- TPL-14 fixed (template `ww.rs` on the current Context / EventWriter API).
- Template: crash capture on by default on Cortex-M (`--no-crash-capture`), build id in every Cortex-M app,
  `src/last_reset.rs`, HardFault / panic handlers, `FirmwareInfo` from `<Root>Consts` (API-36 landed on master as
  2ebdfb16) and `LastReset` handlers with `--wire-weaver`.
- FEATURES: ROB-11 (🧪), ROB-12 (🐛 flip-link never used), ROB-1 → 🐛, DEV-2 → 🧪, TPL-14 ✅. CHANGELOGs under
  `[Unreleased]`.

## Decisions

- No WireWeaver dependency in `bedrock_crash`: firmware takes bedrock from git and wire_weaver from a checkout
  (usb_io_fw, typec_dongle), a ww dependency inside bedrock would not resolve or would duplicate wire_weaver. The
  generated `last_reset.rs` converts to `ww_firmware_info::ResetRecord`.
- The record is never read after power-on / brown-out (H7 ECC RAM); a pending crash beats the register's cause
  (RP doesn't record SYSRESETREQ). A crash stays until `clear()`; `cause` is always the last reset's.
- STM32 reset flags read as a raw address with bits from stm32-data: one code path for all four frameworks.
- Panic with a debugger attached: `udf` (probe-rs stops, like panic-probe); HardFault: always `sys_reset`.

## Open / next

- Hardware check (no Pico 2 / H7 probe on the mesh): force a HardFault and a panic on a Pico 2 and an H7, read
  `LastReset` after the reset, symbolize `pc` / stack with the ELF. Check there: RP235x `CHIP_RESET` after
  `SYSRESETREQ`, H7 ECC with the record, the rp2350 boot ROM leaving the `.uninit` area alone.
- ROB-12: flip-link fix (`linker = "flip-link"` in `.cargo/config.toml`), separate change with upgrade notes.
- nRF `RESETREAS`, RISC-V `Fault::RiscV`, a timer for uptime outside embassy.
- usb_io_fw adoption (piece 7): its `init_ram.rs` must leave `.uninit` alone.
- `bedrock_crash` from git (`--bedrock git`, the default) works only after this lands and is pushed.

## Gotchas

- `cargo clippy --workspace` fails in `bedrock_build_info` (redundant_slicing), from before this branch.
- Matrix runner used for this session: `$CLAUDE_JOB_DIR/tmp/matrix.py` (throwaway), builds into `/tmp/bcc/m`.
