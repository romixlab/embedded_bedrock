# PWR checks, explained

Detail page for the `PWR` category of the
[check catalogue](../checks.md#-pwr---supply-supervision-and-low-power). Each section explains one check: what it
catches, why the firmware fails without it, a concrete example, how it is checked, and the fix. The one-line
entries in the catalogue stay the source of truth for ID, severity and wording; this page is the commentary.

## `PWR-1` No brown-out level or PVD for the board's real supply <a id="pwr-1"></a>

⚠️ warning · check: src easy · [catalogue entry](../checks.md#pwr-1)

**What it catches.** No brown-out reset threshold configured (or no programmable voltage detector set up on parts
that need one explicitly) matching the supply the board actually has.

**Why it matters.** Without a brown-out level matched to the real supply, a sagging rail runs the MCU below the
voltage its flash and peripherals are specified for instead of resetting it; the failure that follows (a flash
write that doesn't take, a peripheral that misbehaves) looks unrelated to power at all, because nothing flagged
the sag as the cause.

**How to check.** `src easy`: the brown-out/PVD configuration call must be present in `init()` and its threshold
compared against the board's nominal supply voltage from the board record.

**Fix.** Set the brown-out threshold (option bytes, or PVD) to a level safely above the board's minimum operating
voltage, not left at the chip's factory default if that doesn't match the board.

## `PWR-2` H7 SMPS/LDO config not matching the schematic <a id="pwr-2"></a>

🛑 error · check: cfg mid + review · chip: H7 · [catalogue entry](../checks.md#pwr-2)

**What it catches.** The same `supply_config` setting `BOOT-2` checks against the ROM's lock, checked here against
the schematic directly: `Default`, `LDO`, `DirectSMPS`, `SMPSLDO(...)`, `SMPSExternalLDO(...)`,
`SMPSExternalLDOBypass(...)` or `SMPSDisabledLDOBypass` must match how the board actually wires VCORE (per
`stm32-notes.md`'s SMPS table).

**Why it matters.** This is the one place in the whole template where getting a setting wrong can damage the
part, not just malfunction: feeding `DirectSMPS` into a board wired for `LDO` (or the reverse) can put the wrong
voltage or topology on VCORE entirely, which is why `bedrock_gen.py` refuses to generate H7 firmware without
`--supply-config` set explicitly.

**Example.** `stm32-notes.md`'s table exists specifically because this has to come from the schematic, not a
default — "Ask the user for the schematic; the script refuses to generate without `--supply-config`."

**How to check.** `cfg mid` (the generator's own refusal to proceed without the flag) plus `review` (confirming the
chosen variant actually matches the board's VCORE wiring, which no amount of config parsing alone can verify).

**Fix.** Read the schematic's VCORE supply topology and set `--supply-config` (and `embassy_stm32::Config`) to
match exactly; when in doubt, ask rather than guess (Default/LDO is the safe fallback on boards without SMPS, e.g.
most Nucleo boards).

## `PWR-3` Backup domain not reset when RTC/backup registers are unused <a id="pwr-3"></a>

⚠️ warning · check: src mid · [catalogue entry](../checks.md#pwr-3)

**What it catches.** A firmware that uses neither the RTC nor backup registers, but never calls
`reset_backup_domain()` (pulsing `RCC.BDCR.BDRST`) at boot.

**Why it matters.** The backup domain can be corrupted by a brown-out that isn't a full power-on reset, leaving
the LSE oscillator starting itself unexpectedly, or pins like PC13-PC15 in the wrong mode — both are efton.sk
gotchas (g133, g62) specific to this domain's odd reset behaviour, and resetting it deliberately at boot is the
template's own default precisely because nothing else clears it reliably.

**Example.** `stm32-notes.md` "Backup domain": `src/init.rs::reset_backup_domain()` is generated to pulse `BDRST`
at boot exactly when `--rtc` isn't requested.

**How to check.** `src mid`: when the firmware model shows no RTC and no `--bkp-counters` use, `init()` must call
the backup-domain reset.

**Fix.** Call `reset_backup_domain()` (pulse `BDCR.BDRST`) early in `init()` whenever the RTC and backup registers
are genuinely unused; skip it only when something in the backup domain (counters, RTC) needs to survive resets.

## `PWR-4` RTC clock source changed while backup-domain state must survive <a id="pwr-4"></a>

⚠️ warning · check: src mid · [catalogue entry](../checks.md#pwr-4)

**What it catches.** A clock-configuration change that alters the RTC's clock source, in a firmware that also
keeps state in the backup domain (`--bkp-counters`, or RTC-backed timekeeping) across resets.

**Why it matters.** Changing the RTC clock source is exactly the kind of backup-domain change embassy itself
handles by resetting the domain — which, per `PWR-3`'s logic, is the right move when nothing needs to survive, and
the wrong move here: it wipes the counters or time state the firmware was relying on surviving the reset that just
happened.

**Example.** donguru_template's `main.rs` needed this worked out explicitly: a clock-config edit that touched the
RTC source reset backup-domain state nobody meant to lose.

**How to check.** `src mid`: a clock-config change affecting the RTC source, in a firmware model that also shows
backup-register or RTC-backed persistent use, is the pattern to flag.

**Fix.** Keep the RTC clock source stable once the firmware starts relying on backup-domain persistence; if it
must change, treat it as a deliberate one-time reset of that state, not a side effect to discover later.

## `PWR-5` Run-on time after a supply cut not measured <a id="pwr-5"></a>

⚠️ warning · check: bench · [catalogue entry](../checks.md#pwr-5)

**What it catches.** No measurement of how long the MCU keeps running (on bulk capacitance, a supply's hold-up
time) after its input supply is physically removed — specifically at the moments that matter: during a flash
write or erase, or a settings commit.

**Why it matters.** `FLS-6` and `BOOT-5` both assume a power-cut test exercises the *real* timing a board gives
the firmware to react; without measuring it, a fuzz test that assumes "plenty of time" or "none at all" may not
match what the hardware actually provides, and the firmware's power-loss handling is tuned against the wrong
number.

**Example.** A B129 G0 board measured 60-70 ms of run-on after a supply cut (internal measurement log, Oct 2026) — long enough to
matter for in-flight writes, and not a number that could have been guessed from the schematic alone.

**How to check.** Not automated (`bench`): cut the supply under a scope or logic analyser at the moments that
matter and measure how long the core keeps executing.

**Fix.** N/A — this *is* the check; the measured number feeds into how much work `FLS-1`/`FLS-6`'s power-safe
paths can assume they have time for.

## See also

- [Bring-up checklist](../bringup-checklist.md) step 6 is `PWR-5` directly, alongside `FLS-6` and `BOOT-5`.
- [stm32-notes.md](../../firmware_template_skill/references/stm32-notes.md) "SMPS / SupplyConfig", "Backup
  domain" — the source for `PWR-2` through `PWR-4`.
- [`BOOT-2`](boot.md#boot-2) — the ROM-lock side of the same H7 supply-config setting `PWR-2` checks against the
  schematic.
