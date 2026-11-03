# FLT checks, explained

Detail page for the `FLT` category of the [check catalogue](../checks.md#-flt---faults-panics-and-resets). Each
section explains one check: what it catches, why the firmware fails without it, a concrete example, how it is
checked, and the fix. The one-line entries in the catalogue stay the source of truth for ID, severity and wording;
this page is the commentary. `bedrock_crash` is the crate behind most of this category: it carries a fault/panic
record and the boot counter across a reset.

## `FLT-1` HardFault handler that spins <a id="flt-1"></a>

🛑 error · check: src easy · [catalogue entry](../checks.md#flt-1)

**What it catches.** A `HardFault` handler whose body is (or reduces to) `loop {}`, in a build meant to ship.

**Why it matters.** A spinning fault handler turns every fault into a silent, permanent hang: no reset, no record,
nothing visible on the outside except a device that stopped responding. The whole point of a fault is that it's
information about a bug; spinning throws that information away and gives the user a brick instead of a reboot.

**Example.** cortex-m-rt's template default is exactly this `loop {}`; embedded_bedrock ROB-2/ROB-11, usb_io_fw
PLAT-7, and the Notion firmware checklist all call out replacing it before shipping.

**How to check.** `src easy`: the `HardFault` handler body must not be an empty or infinite loop with no exit.

**Fix.** Record what's known about the fault (`bedrock_crash`, raw — `FLT-2`) and reset; never leave the default
spin in a shipped build.

## `FLT-2` Anything that can panic or nest-log inside a fault handler <a id="flt-2"></a>

🛑 error · check: src easy · [catalogue entry](../checks.md#flt-2)

**What it catches.** defmt logging, `cnt!`, `unwrap`, or any other call that can itself panic or trigger another
log, reached from inside `HardFault` or `DefaultHandler`.

**Why it matters.** A fault handler already runs in the worst-understood state the firmware can be in; anything
inside it that can fail recurses straight into the same handler (or a different fault) with no bottom to the
recursion, which turns a diagnosable fault into a true hang with no record at all.

**Example.** typec_dongle FW-48: a `DefaultHandler` that tried to log hit exactly this, which is also why `IRQ-8`'s
default handler must only record the raw IRQ number.

**How to check.** `src easy`: reachability from the handler body to any call that formats, allocates, or can
itself panic.

**Fix.** Record only raw register values and numbers (no formatting, no logging) inside `HardFault`/
`DefaultHandler`; do formatting and logging only after a reset, reading the crash record back.

## `FLT-3` `exc_return!()` not the first statement of HardFault <a id="flt-3"></a>

⚠️ warning · check: src easy · [catalogue entry](../checks.md#flt-3)

**What it catches.** `bedrock_crash::exc_return!()` called anywhere in the `HardFault` handler other than as its
very first statement.

**Why it matters.** `exc_return!()` reads the link register to recover the exception return information the crash
record needs; any code that runs before it (even an apparently harmless local variable) can change what's in `LR`
by the time it's finally called, corrupting the one piece of context that says what the CPU was doing when the
fault happened.

**Example.** The template's generated `main.rs` places the macro as literally the first line of the handler for
exactly this reason.

**How to check.** `src easy`: a syntactic check that `exc_return!()` is the first statement in the function body.

**Fix.** Move the call to the top of the handler, before anything else, including seemingly-inert local bindings.

## `FLT-4` Missing UsageFault/MemManage/BusFault handlers <a id="flt-4"></a>

⚠️ warning · check: src easy · chip: M3+ · [catalogue entry](../checks.md#flt-4)

**What it catches.** A core that has dedicated UsageFault, MemManage and BusFault exceptions (Cortex-M3 and above)
left without handlers for them, or without the SHCSR bits that route them to their own handler instead of
collapsing into HardFault.

**Why it matters.** Without the dedicated handlers (and SHCSR enabled), every one of these faults escalates to a
plain HardFault, which still works but throws away exactly the information (which of the three, and the
fault-specific status registers) that would otherwise narrow down the cause immediately.

**Example.** embedded_bedrock ROB-4 established this as a template default on cores that support it.

**How to check.** `src easy`, chip M3+: the three handlers must be present and SHCSR's enable bits set before
`main()` runs.

**Fix.** Enable the three handlers and their SHCSR bits early in `init()`; record each one's own fault status
register (not just a generic HardFault record) via `bedrock_crash`.

## `FLT-5` Reset cause not read and reported <a id="flt-5"></a>

⚠️ warning · check: src easy · [catalogue entry](../checks.md#flt-5)

**What it catches.** Boot code that never reads the chip's reset-cause register (power-on, brown-out, watchdog,
software, pin, lockup) and never reports it anywhere (log, build-info counters, the field).

**Why it matters.** The reset cause is free information the hardware already has at boot; not reading it throws
away the one signal that tells "did this reboot because of a crash, a watchdog timeout, or just a normal power
cycle" — exactly the distinction `FLT-10`'s bring-up test exists to confirm works.

**Example.** The Notion firmware checklist and embedded_bedrock ROB-11 both call this out as a baseline every
firmware should have.

**How to check.** `src easy`: `init()` must read the reset-cause register and store or log the result before it's
cleared for the next reset.

**Fix.** Read the reset-cause register first thing in `init()`, before anything else can clear it, and surface it
(defmt at boot, a build-info field, or both).

## `FLT-6` Crash record read without the power-on/magic+CRC guard <a id="flt-6"></a>

🛑 error · check: review · [catalogue entry](../checks.md#flt-6)

**What it catches.** Code reading `bedrock_crash`'s record (or any RAM handoff data) without first checking the
reset cause wasn't power-on/brown-out, and without checking a magic value plus CRC on the record itself.

**Why it matters.** RAM content after a power-on or brown-out reset is architecturally undefined (and worse on
parts with ECC RAM, where an uninitialised word can itself fault on read); reading a "crash record" that's really
just power-up noise produces a plausible-looking but meaningless report, which is worse than no report because it
looks trustworthy.

**How to check.** Not automated (`review`): confirm the read path checks reset cause, magic, and CRC, in that
order, before trusting any field in the record.

**Fix.** Gate every crash-record read on "reset cause wasn't power-on/brown-out" first, then a magic value, then a
CRC over the record; `bedrock_crash` provides this — don't bypass it with a raw read of the backing memory.

## `FLT-7` No watchdog, or one fed unconditionally <a id="flt-7"></a>

🛑 error · check: src mid · [catalogue entry](../checks.md#flt-7)

**What it catches.** No watchdog configured at all, or one fed from an interrupt handler, or fed from a loop with
no bound on how long it can run, or with a window that was never calibrated against the chip's real clock
tolerance.

**Why it matters.** A watchdog fed from an interrupt keeps getting fed even if the task it's meant to supervise
has hung — the interrupt doesn't know or care whether thread-mode work is actually progressing, so the one failure
mode watchdogs exist to catch (the main flow got stuck) is exactly the one an interrupt-fed watchdog can't see.

**Example.** The generic firmware checklist (Notion import) flags this as a recurring bring-up gap across
projects.

**How to check.** `src mid`: the watchdog feed call must be reachable only from a path that implies the main
task/loop actually completed real work since the last feed, not from an interrupt handler or an unconditional
timer callback.

**Fix.** Feed the watchdog from the main flow, after work that proves forward progress, never from an interrupt;
calibrate the window against the chip's actual clock tolerance, not its nominal one.

## `FLT-8` No boot-loop rescue <a id="flt-8"></a>

⚠️ warning · check: src easy · [catalogue entry](../checks.md#flt-8)

**What it catches.** No counter, in a backup register, of boots since the last time the firmware marked itself
healthy — and so no fallback into the ROM bootloader after a few unhealthy boots in a row.

**Why it matters.** A firmware that crashes before it can mark itself healthy (a bad update, a new regression)
boot-loops forever with no rescue path other than a debug probe; a counted fallback into the ROM bootloader after
a few failed boots turns "the board is bricked" into "the board is recoverable over USB".

**Example.** typec_dongle FW-45, and embedded_bedrock DEV-16 which specifies the device-wide boot-loop rescue
contract every board should meet.

**How to check.** `src easy`: a backup-register boot counter incremented at boot and cleared on a healthy mark,
with a check that falls into the ROM bootloader past a threshold.

**Fix.** Count boots in a backup register, clear it once the firmware marks itself healthy, and jump to the ROM
bootloader (or hold there) once the count passes a small threshold (DEV-16's contract).

## `FLT-9` Healthy mark never set on the transport in use <a id="flt-9"></a>

⚠️ warning · check: src mid · [catalogue entry](../checks.md#flt-9)

**What it catches.** A healthy-mark condition (like `usb_configured()`) that's never actually reached because the
firmware's real transport isn't the one the condition checks — a build using Ethernet or CAN with a healthy mark
that only ever fires on USB enumeration.

**Why it matters.** With the mark never set, `FLT-8`'s boot-loop rescue thinks every boot is unhealthy and the
firmware falls into the ROM bootloader after a few boots regardless of whether anything is actually wrong — every
boot waits out the full rescue timeout for no reason.

**Example.** embedded_bedrock TPL-64 found exactly this mismatch between the configured transport and the
condition the generated healthy-mark code checked.

**How to check.** `src mid`: cross-reference which transport(s) the firmware actually enables against which
condition the healthy-mark call is gated on.

**Fix.** Gate the healthy mark on a condition that's reachable for the transport actually in use, or mark healthy
on a transport-independent milestone (first successful main-loop iteration) if no single transport condition fits.

## `FLT-10` Forced HardFault and panic never tried on hardware <a id="flt-10"></a>

🛑 error · check: bench · [catalogue entry](../checks.md#flt-10)

**What it catches.** A fault/panic reporting path that's never actually been exercised on real hardware — only
reasoned about from the code.

**Why it matters.** Every other `FLT` check is a static property of the code; whether the whole chain actually
works (fault happens, `bedrock_crash` records it, the reset-cause-gated read on the next boot reports it correctly
with the ELF symbolized) is something only a real fault on real hardware proves.

**Example.** embedded_bedrock ROB-11: deliberately forcing both a HardFault and a panic was part of establishing
that the reporting path actually works end to end, not just compiles.

**How to check.** Not automated (`bench`): force both a HardFault and a panic on target, reset, and confirm the
next boot reports "last reset: Fault / Panic" with symbols resolved.

**Fix.** N/A — this *is* the check; a failure here means going back through `FLT-1` through `FLT-6`.

## `FLT-11` `exc_return` relied on for crash capture on Cortex-M0+ <a id="flt-11"></a>

💡 suggestion · check: review · chip: M0+ · [catalogue entry](../checks.md#flt-11)

**What it catches.** Crash-capture code that reads `exc_return` for context on a Cortex-M0+ core, where that
value reads as 0 (M0+ doesn't expose it the way M3+ does).

**Why it matters.** Code written and tested against an M3+ part and then reused on an M0+ target keeps compiling
and keeps "working" — it just silently records nothing useful for that one field, which is easy to miss since
nothing errors.

**Example.** typec_dongle FW-13 hit this directly when crash-capture code shared across chip families assumed
`exc_return` behaved the same everywhere.

**How to check.** Not automated beyond flagging the chip (`review`): confirm the crash record's `exc_return`-based
fields are either omitted or clearly marked unavailable on M0+ builds.

**Fix.** Don't rely on `exc_return` on M0+; use the fields that are actually available there (stacked registers,
IPSR) for whatever context is needed.

## `FLT-12` Runaway conditions not handled (AN1015) <a id="flt-12"></a>

⚠️ warning · check: review · [catalogue entry](../checks.md#flt-12)

**What it catches.** Control registers of critical peripherals that are written once at init and never
re-written or periodically checked, and unused flash left with its erased value instead of illegal-opcode bytes.

**Why it matters.** AN1015's class of EMC-induced upset (a bit flip in a control register from a transient) isn't
caught by a fault handler, because nothing faults — the peripheral just silently runs with the wrong
configuration from then on; periodically re-asserting critical config, and filling unused flash with an
illegal/trap opcode, turns a silent misconfiguration into either a self-correction or a fault.

**How to check.** Not automated (`review`): AN1015-class hardening is a design posture, not a single pattern to
grep for.

**Fix.** Periodically re-write (or at least re-check) critical peripheral configuration from a known-good copy;
fill unused flash with a trap instruction rather than leaving it erased.

## `FLT-13` Critical data used without a validity check <a id="flt-13"></a>

⚠️ warning · check: review · [catalogue entry](../checks.md#flt-13)

**What it catches.** Data the firmware treats as critical (calibration, a safety limit, a configuration loaded
from flash or backup registers) used directly, with no CRC, redundant copy, or range check before trusting it.

**Why it matters.** Flash and RAM both have failure modes that corrupt a value without the firmware ever faulting
(a torn write, ECC's own undefined-until-written state, the EMC upset `FLT-12` describes); for data that controls
something critical, using it unchecked means a single corrupted word can drive the wrong behaviour with nothing
anywhere noticing.

**How to check.** Not automated (`review`): identify data the project considers critical, and confirm it's
checked (CRC, redundancy, or a plausible-range check) before use, not just loaded and trusted.

**Fix.** Add a CRC or redundant copy to critical data at rest, and a range/sanity check at the point of use;
reject and fall back to a safe default rather than acting on an unchecked value.

## See also

- [Review checklist](../review-checklist.md) — *Faults* condenses FLT-1 through FLT-5, FLT-7 through FLT-9,
  FLT-12 and FLT-13.
- [Bring-up checklist](../bringup-checklist.md) step 4 is `FLT-10` and `STK-7` together; step 5 exercises
  `FLT-8`.
- [`IRQ-8`](irq.md#irq-8) — the same "nothing that can panic" rule for `DefaultHandler`.
- `bedrock_crash`'s own README for the record format `FLT-6` checks the magic and CRC of.
