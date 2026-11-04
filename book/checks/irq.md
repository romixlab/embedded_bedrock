# IRQ checks, explained

Detail page for the `IRQ` category of the
[check catalogue](../checks.md#-irq---interrupts-and-critical-sections). Each section explains one check: what it
catches, why the firmware fails without it, a concrete example, how it is checked, and the fix. The one-line entries
in the catalogue stay the source of truth for ID, severity and wording; this page is the commentary.

## `IRQ-1` Interrupt flags cleared wholesale <a id="irq-1"></a>

🛑 error · check: src mid · [catalogue entry](../checks.md#irq-1)

**What it catches.** A status-register clear that writes all-ones (or writes back the value it just read) instead
of exactly the flags the handler is about to act on.

**Why it matters.** A W1C (write-1-to-clear) register clears every bit written as 1, not just the ones the code
cared about; an event flag that sets between the read and the wholesale clear is lost without the handler ever
seeing it. On a ring-style peripheral that means a dropped element nothing else notices.

**Example.** mcan finding I1/Y6: clearing the whole interrupt status register lost a pending RFxL (Rx FIFO lost
message) flag that arrived in the gap between the read and the clear.

**How to check.** `src mid`: needs the register's field layout to tell "clear everything" from "clear exactly the
bits handled"; a literal all-ones write or a write of the just-read value is the pattern to flag.

**Fix.** Clear only the bits the handler is about to act on, computed from what it actually read and handled, not a
blanket mask.

## `IRQ-2` W1C flags cleared with `modify()` <a id="irq-2"></a>

🛑 error · check: src mid (needs SVD access types) · [catalogue entry](../checks.md#irq-2)

**What it catches.** A `rc_w1` / W1C status register cleared through a read-modify-write `modify()` call instead of
a direct write of the one bit meant to clear.

**Why it matters.** `modify()` reads the register, flips the bit in software, and writes the result back; for a
W1C register every other bit that's 1 in that read-back value gets written back as 1 too, which clears it — so any
flag that set between the read and the write is wiped along with the one the code meant to clear.

**Example.** embassy-stm32 0.6 on G0: the ADC interrupt handler used `modify()` on the ISR register and wiped CCRDY
(calibration-ready) whenever it set in that same window (internal log, Oct 2026).

**How to check.** Needs the register's access type from the chip's SVD/data (`rc_w1`) to tell a legitimate
read-modify-write register apart from one that must only ever be written, never read-modified.

**Fix.** Write the one bit directly (a bare write of the bit's mask), never through `modify()`, on any register the
chip data marks `rc_w1`.

## `IRQ-3` Handler that doesn't check its source flag, or clears it late <a id="irq-3"></a>

⚠️ warning · check: review · [catalogue entry](../checks.md#irq-3)

**What it catches.** A handler on a shared or multi-cause vector that acts without first checking which flag fired,
or that clears its flag after doing the work rather than before (or atomically with) it.

**Why it matters.** Acting without checking the source misattributes the interrupt's cause on a multi-cause vector;
clearing late leaves a window where a second, real event arrives before the clear and is thrown away as if it were
the first one still being handled (a double interrupt collapsed into one).

**How to check.** Not automated (`review`): read the handler for an explicit flag check before acting, and for
where the clear sits relative to the work.

**Fix.** Check the flag first; clear it as early as the hardware allows (ideally before doing anything that could
take long enough for a second event to arrive).

## `IRQ-4` Several drivers on one shared vector, one disables it on drop <a id="irq-4"></a>

🛑 error · check: src mid, chip table · [catalogue entry](../checks.md#irq-4)

**What it catches.** More than one driver bound (via `bind_interrupts!` or by hand) to the same NVIC vector, where
one of them disables "its" interrupt in its `Drop` impl.

**Why it matters.** The NVIC vector, not the peripheral, is what gets masked; disabling it on one driver's drop
masks every other driver sharing that vector too, which then goes silently deaf with no error anywhere near the
code that caused it.

**Example.** The STM32G0B1's combined `USB_UCPD1_2` vector (typec_dongle API-30): a driver that disabled the
vector when done with UCPD also silenced the USB interrupt sharing it.

**How to check.** `src mid` plus a chip table of which peripherals share a vector on which part: flag a `Drop` impl
that disables an interrupt line also claimed by another bound peripheral on the same vector.

**Fix.** Never disable a shared vector from one driver's drop; disable only the peripheral's own enable bit, or
reference-count the vector's mask across the drivers that share it.

## `IRQ-5` Interrupt line routed but not enabled, or enabled with no handler <a id="irq-5"></a>

⚠️ warning · check: src mid · [catalogue entry](../checks.md#irq-5)

**What it catches.** A peripheral's interrupt line selected into the NVIC (ILS-style routing) without the line also
being enabled (ILE), or enabled with no `bind_interrupts!` handler bound to the vector it reaches.

**Why it matters.** Routed-but-not-enabled silently never fires: the peripheral looks configured for interrupts but
the event never reaches the core. Enabled-with-no-handler is worse on cores whose default handler loops or resets —
the first real event becomes a fault instead of the feature it was meant to support.

**Example.** mcan finding I2: a line selection without the matching enable bit, caught during review of the
interrupt configuration.

**How to check.** `src mid`: cross-reference the peripheral's routing/select bits against its enable bits and
against the set of vectors with a bound handler.

**Fix.** Enable every line that's routed, and bind a handler to every vector that's enabled; remove routing for
anything genuinely unused.

## `IRQ-6` Nested disable/enable of interrupts by hand <a id="irq-6"></a>

⚠️ warning · check: src easy · [catalogue entry](../checks.md#irq-6)

**What it catches.** Code that disables interrupts, calls into something else that also disables and later
re-enables them, and ends up with interrupts turned back on while the outer section still expected them off.

**Why it matters.** Hand-rolled disable/enable pairs don't nest: an inner `enable()` turns interrupts back on
unconditionally, even while an outer section is still relying on them staying off, reopening exactly the race the
outer section existed to prevent.

**How to check.** `src easy`: a pattern match for interrupt enable/disable calls that aren't going through
`critical_section::with` (which nests correctly by construction).

**Fix.** Use `critical_section::with(|cs| { ... })` for every critical section instead of manual
disable/enable pairs.

## `IRQ-7` Interrupt handler doing more than capture <a id="irq-7"></a>

💡 suggestion · check: elf mid · [catalogue entry](../checks.md#irq-7)

**What it catches.** A handler body that formats data, encodes a message, or logs, instead of doing the minimum —
a few register reads, a few stores, maybe a counter — and leaving the rest to task context.

**Why it matters.** Everything in a handler runs at that interrupt's priority, blocking anything of equal or lower
priority for as long as it takes; formatting and encoding are exactly the kind of variable-length, potentially slow
work that turns an interrupt's latency budget into something unpredictable.

**How to check.** `elf mid`: look at what the handler's compiled code actually calls into (formatting machinery,
encoders) rather than trusting the source shape alone.

**Fix.** Capture raw data and signal a task; do formatting, encoding and logging there, outside interrupt context.

## `IRQ-8` Unused interrupt vector not caught by the default handler <a id="irq-8"></a>

⚠️ warning · check: src easy · [catalogue entry](../checks.md#irq-8)

**What it catches.** A `DefaultHandler` that doesn't record which IRQ number actually fired, or that tries to log
from inside itself.

**Why it matters.** An interrupt landing on a vector nothing expects (a mis-configured peripheral, a stray enable
left on, hardware the board doesn't have asserting a line) is exactly the kind of thing worth knowing about after
the fact; a `DefaultHandler` that doesn't capture the IRQ number throws that information away, and one that logs
risks nesting into whatever caused the stray interrupt in the first place (`FLT-2`).

**Example.** typec_dongle FW-48: a `DefaultHandler` that tried to log ran into the same nested-panic risk `FLT-2`
exists for.

**How to check.** `src easy`: the `DefaultHandler` body must record the active IRQ number (e.g. into a counter or a
crash record) and must not call anything that can panic, allocate, or log.

**Fix.** Record the IRQ number raw (a `cnt!` counter indexed by vector, or a crash-record field); never format or
log from inside it.

## See also

- [Review checklist](../review-checklist.md) — *Interrupts* condenses IRQ-1 through IRQ-4 and IRQ-6, IRQ-7 into one
  pass.
- [`EXE`](exe.md) — interrupt executors are the other half of "what runs at interrupt priority".
- `FLT-2` ([FLT checks](flt.md#flt-2)) — the same nothing-that-can-panic rule applies inside `DefaultHandler`
  (`IRQ-8`) as inside `HardFault`.
