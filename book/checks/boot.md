# BOOT checks, explained

Detail page for the `BOOT` category of the [check catalogue](../checks.md#-boot---boot-rom-bootloader-and-updates).
Each section explains one check: what it catches, why the firmware fails without it, a concrete example, how it is
checked, and the fix. The one-line entries in the catalogue stay the source of truth for ID, severity and wording;
this page is the commentary.

## `BOOT-1` Entry by jump not detected <a id="boot-1"></a>

🛑 error · check: src mid · [catalogue entry](../checks.md#boot-1)

**What it catches.** Firmware entered by a jump from the ROM DfuSe bootloader (or another bootloader) that doesn't
detect it was entered this way and reset once before continuing — VTOR, the PLL configuration, SysTick, and
whatever IRQs the ROM left enabled are all still set from the ROM's own run.

**Why it matters.** `cortex-m-rt`'s startup assumes a clean reset: VTOR at its default, no clocks already running,
no interrupts already enabled. Entered by jump instead, the application's own init code runs on top of leftover
ROM state, which can misconfigure a clock tree that assumes "nothing is running yet" or take an interrupt the
application hasn't set up a handler for.

**Example.** vb135 BOOT-6 and typec_dongle FW-12 both hit this: the application booted fine from a cold reset and
misbehaved only right after a ROM DFU session handed control over by jump.

**How to check.** `src mid`: `init()` must check for ROM-leftover state (VTOR not at its default, SWS bits in
RCC_CFGR showing the wrong clock source, or NVIC active-and-pending state inconsistent with a fresh boot) and force
a real reset if found.

**Fix.** Detect the jump-entry signature (RCC_CFGR SWS, VTOR, NVIC) early in `init()` and issue one software reset
before anything else runs, so the application always starts from a clean reset either way.

## `BOOT-2` H7 supply config written blind <a id="boot-2"></a>

🛑 error · check: src mid · chip: H7 · [catalogue entry](../checks.md#boot-2)

**What it catches.** `embassy_stm32::Config::rcc.supply_config` written without reading back what the ROM
bootloader already locked into `PWR_CR3`, on an H7 part whose ROM is older than V9.3.

**Why it matters.** `stm32-notes.md`'s SMPS table is explicit that the supply config (LDO, DirectSMPS,
SMPSLDO, ...) **must** match the board's actual wiring or the MCU may not start or may be damaged; an old ROM
bootloader locks `PWR_CR3` until the next full power cycle, so writing a *different* config than what's already
locked in doesn't take effect the way the code assumes — the chip keeps running on the locked config while the
application believes it set a different one.

**Example.** vb135 BOOT-8, and embedded_bedrock TPL-69 which is why the generator refuses to generate H7 firmware
without `--supply-config` matching the schematic (see `stm32-notes.md` "SMPS / SupplyConfig").

**How to check.** `src mid`, chip H7: read `PWR_CR3` before writing supply config; if it's already locked, the
configured value must match what's locked, and a mismatch should be counted, not silently overwritten.

**Fix.** Read `PWR_CR3` back first; if locked to something other than the intended config, that's a power cycle
away from matching — report the mismatch rather than writing blind.

## `BOOT-3` Control signal on a ROM-bootloader pin <a id="boot-3"></a>

🛑 error · check: review (board map) · [catalogue entry](../checks.md#boot-3)

**What it catches.** A firmware-controlled signal (an enable line, a chip select) sharing a pin with one of the
ROM bootloader's own interfaces (per AN2606: USART, USB, CAN, I²C pins the ROM uses).

**Why it matters.** The ROM bootloader expects its interface pins in their default electrical state when it runs;
a firmware signal driving the same pin for its own purposes can interfere with the ROM session exactly when
someone needs the ROM bootloader most — during a recovery after the firmware itself is broken.

**Example.** typec_dongle FW-50: `PA9` doubled as a DUT USB control line and cut power to the device's own USB
mid-DFU session.

**How to check.** Not automated beyond flagging the pin (`review`, against the board map): cross-reference every
firmware-driven pin against the chip's ROM bootloader interface pins for the parts AN2606 lists.

**Fix.** Move the firmware signal off the ROM's interface pins, or make sure the signal's firmware-driven state
during a DFU session is the state the ROM session needs (idle/released), not actively driven.

## `BOOT-4` No probe-less recovery path <a id="boot-4"></a>

🛑 error · check: review · [catalogue entry](../checks.md#boot-4)

**What it catches.** No way to reach either the ROM bootloader (a BOOT pin, `boot reboot --updater`) or a DFU mode
built into the firmware, without a debug probe — including right after a crash at start-up, before the application
has had a chance to run its own recovery logic.

**Why it matters.** This is the device-wide contract embedded_bedrock's AGENTS.md sets for every board (FEATURES.md
area `DEV`): a board that can only be recovered with a probe needs a bench and a cable for every field failure,
which doesn't scale past a handful of units.

**Example.** vb135 USB-2 is the template's own convention for this: a reachable boot path that survives even a
firmware that crashes immediately at boot.

**How to check.** Not automated (`review`): the recovery path (button, header pin, or firmware DFU entry
condition) must be reachable with nothing but the device itself, and must still work if the application never
reaches its main loop.

**Fix.** Wire a BOOT pin to something reachable (button, test pad, connector pin — see `BOOT-9` for how), or give
the firmware a DFU entry condition checked before anything that could crash runs.

## `BOOT-5` Bootloader state written in a tearable order <a id="boot-5"></a>

🛑 error · check: test (power-cut fuzz) · [catalogue entry](../checks.md#boot-5)

**What it catches.** embassy-boot STATE writes ordered so a power cut between them can leave neither the old nor
the new image recognised — specifically, erasing STATE before the magic that marks an image valid is zeroed loses
both.

**Why it matters.** A firmware update is exactly the moment a power cut is both more likely (a field update,
sometimes over a flaky link) and most costly (the device may now have no valid image at all); an ordering that
isn't torn-write-safe turns an unlucky power cut during an update into a bricked device.

**Example.** chunky_mcu_updater's own BOOT-5 finding, and upstream embassy issue #7235, both document this
ordering hazard in embassy-boot's STATE handling directly.

**How to check.** `test`: a power-cut fuzz harness that interrupts the update sequence at every byte-write
boundary and checks the device always recovers to *some* valid, bootable image afterward.

**Fix.** Follow (or patch embassy-boot to follow) an ordering where the old image stays valid until the new one is
fully confirmed — zero the magic only after the data it protects is fully committed, never before.

## `BOOT-6` Confirm/mark write that rewrites unchanged state <a id="boot-6"></a>

⚠️ warning · check: test · [catalogue entry](../checks.md#boot-6)

**What it catches.** A confirm or healthy-mark write that goes ahead and rewrites flash even when the state being
written is identical to what's already there.

**Why it matters.** Every flash write is wear, and on a torn-write-safe design unnecessary writes are also
unnecessary exposure to `BOOT-5`'s hazard — a write that changes nothing still has a window where a power cut can
interrupt it.

**Example.** chunky_mcu_updater BOOT-1 found the mark-healthy path writing on every boot regardless of whether the
mark was already set.

**How to check.** `test`: read current state before writing; a write where old == new should be skippable, and a
test can assert the write is actually skipped.

**Fix.** Read before writing; skip the write entirely when the state already matches what would be written.

## `BOOT-7` Update not tried from the previous released version <a id="boot-7"></a>

🛑 error · check: bench · [catalogue entry](../checks.md#boot-7)

**What it catches.** A release shipped without trying the actual field update path (no probe, just the normal
update mechanism) starting from the *previous* released version, not from whatever the developer happened to have
flashed via probe-rs.

**Why it matters.** `BOOT-8` exists precisely because the wire format can change between versions; the only way to
catch a break in the upgrade path from what's actually in the field is to run that exact transition, old binary to
new, over the real update mechanism.

**Example.** Our internal release checklist makes this an explicit gate before a release ships.

**How to check.** Not automated (`bench`): flash the previous released version, then update to the new one over
the field path, and confirm it succeeds.

**Fix.** N/A — this *is* the check; a failure sends the change back to whatever broke compatibility (often
`BOOT-8`).

## `BOOT-8` Update wire format changed without a version bump <a id="boot-8"></a>

⚠️ warning · check: cfg mid (API snapshots) · [catalogue entry](../checks.md#boot-8)

**What it catches.** A change to the update path's wire format (the `reboot --updater` request, the DFU protocol
framing) that isn't reflected in a version bump the older firmware's handshake can detect.

**Why it matters.** An older firmware in the field has to be able to tell "I don't understand this new request"
from "this request failed" — without a version signal, a format change can make the old firmware either hang
waiting for a response it'll never recognise, or worse, misinterpret the new format as valid and act on garbage.

**Example.** typec_dongle FW-55: an update-path format change shipped without the version bump that would have let
the old firmware refuse gracefully instead of misbehaving.

**How to check.** `cfg mid`: diff the update path's API snapshot against the previous release; any structural
change must come with a version field bump in the same change.

**Fix.** Bump the update protocol's version alongside any wire-format change, and make the older side refuse
cleanly (not hang or misparse) on an unrecognised version.

## `BOOT-9` Dual-bank bootloader passed only `bank1_region` <a id="boot-9"></a>

⚠️ warning · check: cfg mid · [catalogue entry](../checks.md#boot-9)

**What it catches.** A dual-bank STM32's bootloader using `embassy-stm32`'s `Flash::into_blocking_regions()` and
passing only `bank1_region` to `BootLoaderConfig`, while the DFU slot (per `memory-layout.md`'s "dual-bank STM32"
placement) sits in bank 2.

**Why it matters.** `stm32-notes.md` ("Dual bank flash") flags this directly: `bank1_region` alone can't reach a
DFU region in bank 2, so the bootloader fails to read the new image from exactly the region the layout put it in —
a generated TODO left unresolved, not a subtle bug.

**Example.** This is the template's own open TODO in the generated bootloader code, documented in
`stm32-notes.md`.

**How to check.** `cfg mid`: when `memory.x` places the DFU region in bank 2, the bootloader's `BootLoaderConfig`
construction must use `bank2_region` or the whole `Flash`, not `bank1_region` alone.

**Fix.** Pass `bank2_region` (or the combined `Flash`) to `BootLoaderConfig` whenever the DFU slot can fall in
bank 2; resolve the generated TODO rather than leaving it in shipped firmware.

## `BOOT-10` Backup-domain helper that doesn't restore what it found <a id="boot-10"></a>

⚠️ warning · check: src mid · [catalogue entry](../checks.md#boot-10)

**What it catches.** A helper that enables backup-domain write access (RTC clock, `PWR_CR1.DBP`) to do one write,
then turns the RTC clock or `DBP` off afterward unconditionally, instead of restoring whatever state it found
before it ran.

**Why it matters.** If the backup domain was already in use for something else (the RTC running, another
feature's backup registers), turning its enabling bits off "to be tidy" after a one-off write breaks that other
use — the helper's caller has no way to know its "harmless" write just disabled someone else's clock.

**Example.** vb135 BOOT-5/BOOT-6: every boot came up unhealthy, eventually falling into the ROM bootloader after
four boots, traced back to a backup-domain helper leaving the domain in a different state than it found it.

**How to check.** `src mid`: a backup-domain-enabling helper must save the prior state of what it's about to
change and restore exactly that state when done, not an assumed default.

**Fix.** Read the current enable state before changing it; restore that saved state afterward instead of always
disabling.

## See also

- [Review checklist](../review-checklist.md) — *Boot* condenses BOOT-1, BOOT-2, BOOT-4, BOOT-8 and BOOT-10.
- [Bring-up checklist](../bringup-checklist.md) step 5 covers `BOOT-1`, `BOOT-2`, `BOOT-4` and `BOOT-7` directly;
  step 6 covers `BOOT-5`.
- [stm32-notes.md](../../firmware_template_skill/references/stm32-notes.md) "SMPS / SupplyConfig", "Dual bank
  flash" — the source for `BOOT-2` and `BOOT-9`.
- [memory-layout.md](../../firmware_template_skill/references/memory-layout.md) "Flash partitioning" — where the
  DFU slot lands, behind `BOOT-9`.
- embedded_bedrock AGENTS.md "device contract" (FEATURES.md area `DEV`) — the probe-less recovery contract behind
  `BOOT-4`.
