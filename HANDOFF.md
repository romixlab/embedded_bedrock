# Handoff: bedrock-settings (P2528#15oct-2059, TPL-5 settings store)

## Done

- Template: `--settings` / `--settings-pages` replace `--config-page` (answer `config_page` read as `settings`).
  `SETTINGS` region at the end of flash (end of bank 2 on dual-bank), default 8 KiB / at least 2 pages.
  `src/settings.rs` (ww_settings on embassy-stm32 blocking `Flash`; `EccSafeFlash` on G0 / H7 with the G0 NMI
  handler; `BlockingAsync` on F0-F4 / F7). `--wire-weaver`: `label: ww_user_info::UserInfo`, written before
  answering; server task selects on the settings' due time. Region only (with a warning) for `--bootloader`
  (TPL-24), RP / nRF / HAL frameworks (TPL-25), other ECC families (TPL-26).
- Docs: SKILL.md, references/memory-layout.md, frameworks.md matrix, FEATURES.md (TPL-5 🧪, TPL-24..26), template
  CHANGELOG entry with upgrade notes.
- Matrix: 15 entries + 3 bootloaders build, rustfmt clean (scratch run in the job tmp dir).
- wire_weaver (session branch, commit 35cf3131): TR-13 fix, the RTT server panicked on its first request since
  TR-12. CLT-30 filed (client link setup budget too short for slow RTT probes).
- Board (Donguru B153A on omarchy-m1, through its J-Link): generated `bset` firmware (`--settings --wire-weaver
  --ww-transport rtt`), label set to `bench-1` over RTT, reloaded after a reset (boot log: `found: true`,
  `label: bench-1`). Scratch on m1: `/tmp/bset-check` (template copy, wire_weaver copy with the CLT-30
  diagnostic of 40 retries, host tool `th/debug/bset_host get|set|caps`, firmware `tf/.../bset`).

## Next

1. Power cut (needs a person): unplug Donguru's USB from omarchy-m1 for ~5 s, plug back, then
   `probe-rs attach --probe 1366:0105 --chip STM32G0B1CEUx /tmp/bset-check/tf/thumbv6m-none-eabi/debug/bset`
   must log `label: bench-1`.
2. Flash Donguru's own firmware back (typec_dongle main, `fw/donguru`) so the board is as the other sessions
   expect it.
3. TPL-5 to ✅ hardware-checked in FEATURES.md, `git rm HANDOFF.md`, `just test` / `just lint`, `tpm work ready`.

## Gotchas

- The Donguru MCU's USB is behind the on-board hub, which waits for its config over I2C: a generated firmware has
  no USB on this board, test over RTT.
- The J-Link times out ("bulk write timed out") for a few seconds after each client exits: leave gaps.
- `--ww-transport rtt` firmware needs wire_weaver with TR-13.
