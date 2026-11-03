# ⚡ PERF — Speed

Detail for the `PERF` checks of the [check catalogue](../checks.md).

## <a id="perf-1"></a>`PERF-1` — a speed or size claim with no numbers at the opt-level that ships

"It's fast enough" or "that's a lot of flash" without a measurement, at the profile that actually ships, is not
a review finding by itself — but it means the next person (including the one who wrote it) has no way to tell
whether a later change made things worse, or whether the claim was ever true outside a debug build.

**Example:** both sources this check comes from say the same thing in different words. `~/git/tpm/roles/firmware.md`
(the tpm firmware pack) asks for numbers, not impressions, before a speed or size claim goes into a report.
wire_weaver's own `AGENTS.md` is explicit about the two instruments to use and never mix up: "Measure, don't
guess. Instruction counts with `examples_mcu/qemu_icount` (bounded `--max-insns`, `--timeout`, trace from QEMU's
stderr), cycles with DWT `CYCCNT` on real silicon ... Instructions are not cycles: report both as what they are,
with the clock."

**Fix:** before a speed or size claim goes into a commit message, a design note, or a FEATURES.md line, measure
it at the profile that ships (not dev), name the instrument (QEMU instruction count vs DWT cycles on real
silicon vs `llvm-size`/`cargo-bloat` for flash) and the clock, and keep the number next to the claim.

**Detection:** check: review — this is a documentation-quality check, not something a lint can verify against
the binary; a reviewer treats an unsourced "faster"/"smaller" claim as a finding.

## <a id="perf-2"></a>`PERF-2` — 64-bit, i128, or a per-element runtime `%`/`/` on a core without them

Cortex-M0/M0+ has no hardware divider and no 64-bit ALU. Any 64-bit add/multiply, i128 arithmetic, float
formatting, or a modulo/division by a value only known at runtime (not a compile-time power of two) compiles
down to a libcall — `__aeabi_lmul`, `__aeabi_uldivmod`, `__multi3` — that costs tens to hundreds of cycles, and
on a hot path (a ring-buffer index, a per-sample scale) that cost is paid every time.

**Example:** this repo's own `bedrock_led` (DEV-12) and `bedrock_sync` (DEV-13) crates are the positive case,
checked in the disassembly on purpose: `bedrock_led`'s breath/blink easing is "u32 math only (no 64-bit libcalls
on the M0+)", and `bedrock_sync`'s hot path (`shared`, run every LED tick) has "no libcall on the Cortex-M0+ at
opt-level `z` and `3`" for the common case — 64-bit adds and 32-bit products of chunks up to 32 ms — only
falling onto a cold path with two `__aeabi_lmul` for a gap longer than that, and a 64-bit divide once a second
at the steer point, not per tick. The negative case: typec_dongle's `ww_si` unit types cost "~4 / 3.4 KiB" (z/O3)
for `i128` math alone (`ops::scale_i128`, `i128::checked_pow`, `ops::compare`) — measured in
`~/git/tpm/prj/P2534-donguru/design/fw-size-2026-10-19.md`, which calls i128 on a Cortex-M0+ "also slow", not
just big.

**Fix:** keep the hot path in u32 (or fixed-point chunks sized to stay there, as `bedrock_sync` does); reserve
64-bit/i128 math for the cold path (once-a-second steer points, not every tick); replace a runtime `%`/`/` with
a compile-time power-of-two mask or a precomputed reciprocal where the divisor is known ahead of time.

**Detection:** check: elf mid, chip: M0+ — grep the disassembly or the symbol table for `__aeabi_lmul`,
`__aeabi_[u]ldivmod`, `__multi3`, `__udivmoddi4` and attribute each hit to the function that pulled it in; a hit
reachable from a function on a tight per-tick or per-byte path is the finding, a hit only on a once-a-second
cold path (like `bedrock_sync`'s steer) is not.

## <a id="perf-3"></a>`PERF-3` — a RAM-resident function without `#[inline(never)]`

Placing a hot function's code in RAM (`#[link_section = ".data.*"]`, copied there by the runtime's data
initialization) only avoids flash wait states if the function's code actually stays where it was placed. Under
LTO, a small function with only call sites in flash-resident code gets inlined straight back into its callers —
the `.data` copy exists, nothing calls it, and the flash-resident inlined copy is what actually runs.

**Example:** `wire_weaver`'s `ww_lz4` crate (used by typec_dongle's FW-42 UART RX compression, which runs the
encoder "in RAM through ww_lz4's `ram`") marks every hot function with
`#[cfg_attr(all(feature = "ram", target_os = "none"), unsafe(link_section = ".data.ww_lz4"), inline(never))]` —
the `inline(never)` is load-bearing, not decoration: without it LTO would inline the function back into its
flash-resident caller and the RAM placement would do nothing. The crate's own doc comment gives the payoff:
"no flash wait states, about 30% fewer cycles than from flash on an STM32G0 at 64 MHz (1.5 KiB of RAM)."

**Fix:** pair every `#[link_section = ".data.*"]` RAM-function attribute with `#[inline(never)]` on the same
function, so LTO can't undo the placement by inlining it into a flash-resident caller.

**Detection:** check: src easy + elf easy — `src`: grep for `link_section = ".data` without an accompanying
`inline(never)` on the same item; `elf`: confirm in the disassembly that the `.data` copy is actually reached by
a `bl`/`blx` from a caller, not dead weight next to an inlined duplicate in `.text`.

## <a id="perf-4"></a>`PERF-4` — blaming the bus before measuring the software on the hot path

A bitwise CRC loop, a bit-by-bit protocol serializer, or per-edge GPIO wrapper calls (embassy's `Flex`, at
roughly 50 cycles a toggle) can dominate a transfer's time budget well before the bus or the peripheral on the
other end becomes the bottleneck — so "the bus/the SD card/the sink chip is slow" is a hypothesis to measure,
not a conclusion to reach from a slow transfer alone.

**Example:** usb_io_fw's IF-12 found an SD transfer running at 0.6 MB/s over a bus physically capable of far
more (a block takes 0.22 ms on the wire at 18.75 MHz); the first suspect was the card or the link, but the
actual bound was WireWeaver's own bitwise `ww_block::crc32_update` on the data path. typec_dongle's FW-42 and
FW-20 are the same lesson from the other direction: UART RX throughput and bit-banged SWD clock speed were both
software-bound (the compression encoder's own cost, the bit-bang loop's own cycle count) long before the wire
rate was the limit.

**Fix:** measure the software cost of the hot-path primitive in isolation (a CRC table vs the bitwise version,
a batched GPIO write vs per-edge `Flex` toggles) before changing anything on the bus/peripheral side; a
slice-by-16 CRC table or a batched serializer is usually the cheap fix once the software cost is confirmed as
the bound.

**Detection:** check: review — needs a profile or a cycle count attributing time to the specific function, not
just an end-to-end throughput number; a reviewer treats "X is slow, must be the bus/chip" without that
attribution as a finding.

## <a id="perf-5"></a>`PERF-5` — wake cost of an event loop with nothing to do

An `async` event loop built from `select!`/`select4` over several timers and wakers pays a fixed cost every
time any of them fires — re-registering wakers, checking each arm — even on a wake that turns out to have
nothing to do. On a core without a divider or much headroom, that fixed cost can show up directly in
throughput, not just in idle power.

**Example:** typec_dongle's FW-40 traced a 17% small-message throughput regression (64 B host→device,
361-366 kB/s before, 301-303 kB/s after a WireWeaver change) mostly to a generated `ObserveState::next_wake`
that didn't return immediately while nothing was observed, and an `ObserveTable::is_active` that walked every
slot instead of checking a flag; fixing those closed most of the gap (to 343 kB/s at 64 B), but left "~5% short"
attributed specifically to the event loop itself — "`select4` with `wait_until` and `OBSERVE.wait()`, a timer
and two waker registrations per wake, critical sections on the M0" — and that residual was not chased further.

**Fix:** where an event loop's wake cost shows up in throughput (not just idle current), reduce the number of
arms a frequent wake has to re-register against, or replace a per-wake flag check with cheaper bookkeeping (a
dirty bit set at the write site instead of a scan at wake time, as the `is_active` fix did).

**Detection:** check: review — needs a throughput measurement before/after at the message size where the fixed
per-wake cost is proportionally largest (small messages, as in FW-40); a lint can't see this, it's a cost paid
across `select!` arms.
