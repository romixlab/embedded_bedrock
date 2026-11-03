# DMA checks, explained

Detail page for the `DMA` category of the [check catalogue](../checks.md#-dma---dma-and-caches). Each section
explains one check: what it catches, why the firmware fails without it, a concrete example, how it is checked, and
the fix. The one-line entries in the catalogue stay the source of truth for ID, severity and wording; this page is
the commentary.

## `DMA-1` DMA buffer in memory the DMA master can't reach <a id="dma-1"></a>

🛑 error · check: elf mid, chip table · [catalogue entry](../checks.md#dma-1)

**What it catches.** A buffer handed to a DMA transfer that sits in a memory region the chosen DMA master has no
bus access to: H7 DTCM for IDMA, SDMMC or USB OTG; flash for a master that can't read code memory on that part.

**Why it matters.** The DMA engine doesn't fault on an unreachable address the way the CPU would — it either hangs
the transfer or silently returns zeros, which looks like "the peripheral isn't working" rather than "the buffer is
in the wrong RAM bank".

**Example.** vb135's PLAT-5: a buffer placed in DTCM for a master that can't reach it there on H7, where
`main_ram` defaults to AXISRAM for exactly this reason (see [memory-layout.md](../../firmware_template_skill/references/memory-layout.md) "RAM").

**How to check.** `elf mid`, needs a chip table of which DMA masters can reach which memory region: for each DMA
transfer, the buffer's linked address must be inside a region the configured master can access.

**Fix.** Place the buffer in a RAM bank the master can reach (on H7, prefer AXISRAM over DTCM for anything DMA
touches); the template already defaults `main_ram` that way.

## `DMA-2` D-cache on with DMA buffers in cacheable RAM, no clean/invalidate <a id="dma-2"></a>

🛑 error · check: src mid · chip: M7 · [catalogue entry](../checks.md#dma-2)

**What it catches.** D-cache enabled on a Cortex-M7 part with DMA buffers placed in ordinary cacheable RAM, and no
explicit cache clean (before a DMA write) or invalidate (before a DMA read) around the transfer, and no MPU region
marking the buffer non-cacheable.

**Why it matters.** The CPU and the DMA engine don't share a cache; the CPU can read stale cached data after DMA
has written fresh bytes underneath it, or DMA can read stale RAM while the CPU's dirty cache lines haven't been
written back yet. Both look like corrupted or stale data that comes and goes with cache pressure.

**How to check.** `src mid`, chip M7 only: the template's generated `main.rs` leaves D-cache commented out for
exactly this reason (see [stm32-notes.md](../../firmware_template_skill/references/stm32-notes.md) "H7 caches"); a
firmware that turns it on needs either an MPU non-cacheable region over every DMA buffer or explicit clean/invalidate
calls around every transfer.

**Fix.** Put DMA buffers in an MPU region marked non-cacheable, or call the HAL's cache clean/invalidate for the
buffer's address range immediately before/after the transfer.

## `DMA-3` Receive DMA armed only inside an await <a id="dma-3"></a>

⚠️ warning · check: review · [catalogue entry](../checks.md#dma-3)

**What it catches.** A driver (embassy UCPD's `receive()`, a one-shot UART read) that only arms its RX DMA for the
duration of the `.await` the caller is inside — not continuously — so data that arrives between two awaits is
missed rather than buffered.

**Why it matters.** This is the hardware-level version of `EXE-1`'s problem: if the task that owns the receive call
isn't back inside the await promptly, the gap is real dead time where nothing is listening on the line at all.

**Example.** typec_dongle FW-62: embassy UCPD's `receive()` re-arms RX DMA only while awaited; sharing the task
with other work widened the gap enough to overrun RXDR on GoodCRC frames.

**How to check.** Not automated beyond flagging the driver call itself (`review`): whether the gap matters depends
on what else the task does and how tight the protocol's timing is.

**Fix.** Keep the owning task on time (`EXE-1`'s fix: its own interrupt executor), or move to a driver/mode that
uses a continuously-armed ring buffer instead of a one-shot await-scoped arm.

## `DMA-4` DMA ring overrun not counted <a id="dma-4"></a>

⚠️ warning · check: src mid · [catalogue entry](../checks.md#dma-4)

**What it catches.** A DMA ring-buffer consumer (UART RX ring, a sample ring) with no `cnt!` counter incremented on
the overrun path — the case where the producer wraps past data the consumer hasn't read yet.

**Why it matters.** An overrun silently drops data; without a counter there is nothing in the build-info counters
block to show it ever happened, so the first sign of trouble is a user noticing missing data days later with no way
to tell how often it occurs.

**Example.** typec_dongle FW-26 added `pd_rx_overrun` after the fact, once the missing-data reports made clear the
ring had been overrunning unnoticed.

**How to check.** `src mid`: from the firmware model, every DMA ring consumer's overrun branch (the wrap-detected
path) must reach a `cnt!` call.

**Fix.** Add a `cnt!` counter on the overrun branch of every ring consumer, named for what it's counting (see
`CNT-1` for the same rule generalised to every error branch).

## `DMA-5` Per-element modulo in the ring copy on a core without a divider <a id="dma-5"></a>

💡 suggestion · check: elf mid · chip: M0+ · [catalogue entry](../checks.md#dma-5)

**What it catches.** embassy's `ReadableDmaRingBuffer` doing a `%` (modulo) per byte copied out of the ring, run on
a Cortex-M0+ core that has no hardware divider.

**Why it matters.** Every `%` on M0+ compiles to a software division routine (`__aeabi_uidivmod`-class call); done
once per byte at any real data rate it dominates the CPU budget — the P2534 measurement below found 100 % CPU at
only 580 kB/s on a G0.

**Example.** P2534's `upstream-embassy-dma-ringbuffer-copy` finding is exactly this: embassy's ring-buffer copy
loop, unmodified upstream, doing a per-byte modulo on M0+.

**How to check.** `elf mid`: look for calls into the software divide/modulo runtime (`__aeabi_uidivmod` and
relatives) reachable from the ring-buffer copy path, on a core without `PERF-2`'s hardware divider.

**Fix.** Track this as an upstream embassy issue (see `embedded_bedrock` upstream notes) and, until fixed upstream,
size the ring to a power of two and replace the modulo with a mask.

## `DMA-6` Erratum asks for DMA where the code polls <a id="dma-6"></a>

⚠️ warning · check: src mid, chip table · [catalogue entry](../checks.md#dma-6)

**What it catches.** A transfer left in polled/interrupt mode on a part whose errata sheet specifically asks for
DMA to avoid a documented failure — the G0 UCPD TX underflow that then corrupts the next frame's CRC through
`TXHRST`.

**Why it matters.** This isn't a general DMA-vs-polling preference; it's a documented silicon bug where the
non-DMA path has a failure mode the DMA path doesn't, so "it works in testing" doesn't mean the erratum's condition
was ever hit.

**Example.** ES0548 §2.17.1 on the STM32G0's UCPD: a TX underflow is followed by a `TXHRST` that corrupts the
*next* CRC, not just the current frame; the template's `--can`/PD code paths already route this through DMA — see
[errata.md](../../firmware_template_skill/references/errata.md).

**How to check.** `src mid` plus a chip table mapping erratum to affected peripheral/mode: flag a peripheral
configured in a mode the errata table marks "needs DMA" on this chip.

**Fix.** Use DMA for the transfer direction the erratum calls out, per the chip's own errata sheet (`ERR-1`,
`ERR-2`).

## `DMA-7` "Things just hang": suspect DMA into a wrong region first <a id="dma-7"></a>

💡 suggestion · check: review · [catalogue entry](../checks.md#dma-7)

**What it catches.** Not a specific code pattern — a debugging-order reminder: when a board or firmware "just
hangs" with no obvious cause, check for DMA writing into the wrong memory region before chasing anything else.

**Why it matters.** `DMA-1` and `DMA-2` describe two concrete shapes of this, but the generic failure mode (DMA
targeting the wrong address, overlapping something else in use) is common enough, and quiet enough at the symptom
level, that it's worth naming as a first thing to rule out.

**How to check.** Not automated (`review`): a prompt for whoever is debugging, not a pattern the lint can match.

**Fix.** N/A — see `DMA-1`/`DMA-2` for the concrete, checkable versions of this once it's confirmed.

## See also

- [Review checklist](../review-checklist.md) — *DMA* condenses DMA-1, DMA-2, DMA-4 and DMA-6.
- [`EXE-1`](exe.md#exe-1) — the executor-side half of the receive-DMA timing problem (`DMA-3`).
- [memory-layout.md](../../firmware_template_skill/references/memory-layout.md), [stm32-notes.md](../../firmware_template_skill/references/stm32-notes.md) — why `main_ram` defaults away from DTCM on H7, and why D-cache ships off.
