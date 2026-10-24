# Handoff: gen-led-sync (P5 TPL-39 gen-led-breath, then P6 TPL-40 gen-clock-sync)

Plan: ~/git/tpm/maintain/P2528-embedded-bedrock/design/g0-gen-suite.md. Reference: nucleo_g0b1re_template f3bb2a2 (worktree /home/roman/git/.wt/g0-led-sync/nucleo_g0b1re_template, read only), extracted with `git archive` to /tmp/gls/ref (f3bb2a2) and /tmp/gls/ref0 (467f889, before LED+clock). Shell: `export PATH=~/.cargo/bin:~/.local/bin:$PATH`.

Compare: `uv run firmware_template_skill/scripts/bedrock_gen.py new --answers /home/roman/git/.wt/g0-led-sync/nucleo_g0b1re_template/bedrock_fw.json --board nucleo_g0b1re --out /tmp/gls/p5` then `diff -r /tmp/gls/p5 /tmp/gls/ref -x target`.

## Done (wip commit "wip(template): led breath")

- `--led-mode auto|breath|gpio` (default auto), `led_pwm()` in bedrock_gen.py: the pin's TIMx_CHy[N] from stm32-data peripherals[].pins (needs "af": F1 AFIO not generated), plain CH before CHN, general-purpose before TIM_ADV; time driver from TIME_DRIVERS (TIM4, TIM3, then embassy's any order) minus the LED timer. Only STM32 + embassy; else / no channel -> gpio (a note). `o.led_pwm`, suite_context `s.led` (name/Name/var/pin/pwm/essential/timer_mhz) and `s.leds` (ww and breath).
- Preset nucleo_g0b1re: `[led] essential`, `[clock] timer_mhz = 64`.
- Templates: app/src/led.rs.j2 (SimplePwm, or ComplementaryPwm with an inverted Channel impl for CHyN like Donguru), app/src/ww/leds.rs.j2 (G0 verbatim, cnt optional), main.rs mod led + defmt unwrap, embassy-stm32.rs.j2 spawns led_task (led_at_boot -> Mode::On), _embassy_loop (no led arg), ww.rs (no Output, LedsState, led_on/off via crate::led::set), device.rs LEDS_NAMES const, API lib.rs leds ww_impl + docs, API Cargo (ww_indication std/defmt), firmware Cargo (time-driver-timN + comment, bedrock_led, ww_si u32, ww_indication, ww_error without settings), macros bedrock_ww_patch(extra=[...]) patches ww_indication for a local --ww-src, root Cargo ww_indication, client Cargo + client src/leds.rs.j2 (G0 sync.rs's leds half) + lib.rs docs.
- G0 diff of led.rs / ww/leds.rs / main.rs / Cargo.toml: only the clock (P6) and pins (P7) parts remain.

## Next (P5)

1. Build G0 regenerated (firmware + root `cargo test`, clippy), rustfmt check on generated .rs (the `LEDS.run(...)` formatting of led.rs.j2 is a guess; non-ww and CHN variants unchecked).
2. CLI `leds` subcommand (G0 cli main.rs LedsCmd/LedsMode/Level, `Cmd::Leds`), Python leds_mode/brightness/identify (G0 py lib.rs), README "LED" section, suite FEATURES.md.j2 rows, client lib imports (Brightness, Mode in CLI/py use lines).
3. matrix.toml entries: G0 PA5 (TIM2_CH1, ww rtt host), Donguru-like G0B1CE PF1 (TIM15_CH1N, usb), H7 PB0 (TIM3_CH3) and H7 PE1 (no channel -> gpio), G0 PC13 no channel, non-ww breath, --embassy main breath. Note: every embassy STM32 entry with default LED PB14 now breathes (G0 TIM15_CH1, H7 TIM12_CH1, L0 TIM21_CH2, F4 TIM1_CH2N): run the whole embassy part of the matrix.
4. FEATURES TPL-39, template CHANGELOG [Unreleased] + upgrade notes (time-driver, led.rs, API leds = API minor bump), SKILL.md option list. Commit (squash wip).

## Then P6 (TPL-40, --clock-sync)

G0: firmware/src/sync.rs, ww/clock.rs, led.rs run_with SYNC (texts in led.rs doc + `set` doc), Cargo bedrock_sync + ww_timebase + ww_date_time, handler_model clock.steer async, client sync.rs (clock half; G0 puts leds+clock in sync.rs: with clock, render client leds into sync.rs?), CLI clock, py wait_clock_locked/clock_check, examples/clock.py, README clock section. bedrock_ww_patch extra: ("bedrock_sync", "ww_timebase", ...).

## Gotchas

- G0's comments are hand-wrapped at up to 120 cols, inconsistent: rewrap(116) reproduces ww/leds.rs, not led.rs (kept literal there).
- `#[since]` and the API's version-history doc are G0 history, not generated.
- Never touch J-Link 1366:0105; no hardware needed. Nothing to GitHub. `git rm HANDOFF.md` before `tpm work ready`.
