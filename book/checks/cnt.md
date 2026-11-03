# CNT checks, explained

Detail page for the `CNT` category of the [check catalogue](../checks.md#-cnt---counters). Each section explains
one check: what it catches, why the firmware fails without it, a concrete example, how it is checked, and the fix.
The one-line entries in the catalogue stay the source of truth for ID, severity and wording; this page is the
commentary. The `cnt` crate backs this category: counters that survive resets in backup registers (`MEM-5`,
`MEM-6`) and are read back through build info.

## `CNT-1` Error branch without a `cnt!` counter <a id="cnt-1"></a>

⚠️ warning · check: src hard · [catalogue entry](../checks.md#cnt-1)

**What it catches.** An `Err` arm that's handled (not just propagated), a timeout, a retry, or a dropped frame or
overrun, with no `cnt!` counter incremented on that path.

**Why it matters.** This is the generalised version of `DMA-4`'s specific case: any branch that represents
something unexpected happening (not the firmware's normal path) needs to leave a trace somewhere a build-info read
can see, or the only way to learn it's happening at all is a user noticing the downstream symptom and reporting it
— after which there's no history of how often it occurred.

**Example.** The template's own conventions state this directly ("`cnt` counters for unexpected events"), and
typec_dongle's `pd_rx_overrun` (FW-62) is the canonical case of a counter added only after the fact, once missing
data was already being reported.

**How to check.** `src hard`: needs to tell an *unexpected*-event error branch (worth counting) from ordinary,
expected control flow (not worth counting on every single fallible call) — genuinely needs the firmware model and
some judgement, hence the `hard` tier.

**Fix.** Add a `cnt!` counter, named for what it's counting, on every branch that represents something going wrong
rather than a normal outcome; follow `DMA-4`'s ring-overrun pattern as the template.

## `CNT-2` Unaligned or unprotected counter-buffer access <a id="cnt-2"></a>

⚠️ warning · check: src mid · [catalogue entry](../checks.md#cnt-2)

**What it catches.** A `&mut` reference into a counter buffer that an interrupt handler can also update
concurrently, or a `u64` counter accessed without the alignment its update actually needs.

**Why it matters.** A counter updated from both a task and an interrupt handler without synchronisation can lose
increments (two writes racing, one overwriting the other) exactly when it matters most — under the load that's
triggering the events being counted in the first place. An unaligned `u64` access on a core without atomic 64-bit
support can similarly tear between its two halves.

**Example.** Documented in `cnt`'s own CHANGELOG as a fix once the crate started supporting interrupt-context
updates, not just thread-mode ones.

**How to check.** `src mid`: from the firmware model, a counter touched from both interrupt and thread context
needs its update to go through `cnt`'s exclusive-access path (`ldrex`/`strex` on cores that have it, per
`memory-layout.md` "Backup-register counters"), not a plain `&mut` increment.

**Fix.** Update counters only through `cnt`'s own API (which handles the exclusive access correctly where the
core supports it); never take a raw `&mut` into the counter buffer from code that can also run at interrupt
priority.

## `CNT-3` Proc-macro and runtime crate versions not pinned together <a id="cnt-3"></a>

⚠️ warning · check: cfg easy · [catalogue entry](../checks.md#cnt-3)

**What it catches.** `cnt`'s proc-macro crate and its runtime crate present at different versions (a `^` or `~`
range instead of an exact `=x.y.z` pin keeping them locked together).

**Why it matters.** The macro generates code that calls into the runtime's own layout and ABI for a given version;
if Cargo's resolver picks different versions for the two (easy to happen across a workspace with several
dependents), the generated code and the runtime it calls into can silently disagree about the counter buffer's
layout.

**Example.** Noted directly in `cnt`'s own documentation as the reason both crates are versioned and released
together rather than independently.

**How to check.** `cfg easy`: both `cnt` and `cnt-macros` (or however the pair is split) must be pinned to the
exact same version with `=x.y.z` in every `Cargo.toml` that depends on them.

**Fix.** Pin both crates with `=x.y.z` to the same version everywhere they're depended on; bump them together,
never independently.

## See also

- [Review checklist](../review-checklist.md) — *Host input* condenses `CNT-1` alongside the `HOST` checks.
- [memory-layout.md](../../firmware_template_skill/references/memory-layout.md) "Backup-register counters" — the
  storage `CNT-2` is about getting concurrent access right against.
- [`DMA-4`](dma.md#dma-4), [`MEM-5`](mem.md#mem-5), [`MEM-6`](mem.md#mem-6) — the specific counter-placement and
  counter-use checks `CNT-1`/`CNT-2` generalise.
