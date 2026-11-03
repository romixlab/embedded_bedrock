# 🕰️ CLK — Clocks and timers

Detail for the `CLK` checks of the [check catalogue](../checks.md).

## <a id="clk-1"></a>`CLK-1` — Measure the core clock once

Clock-tree math (prescalers, PLL dividers, bus dividers) is easy to get wrong by exactly a factor of two or
three, and nothing in a normal boot log catches it: the firmware keeps running, USB still enumerates, logging
still works, only everything that assumes the core runs at its configured frequency is quietly off. The only way
to be sure the registers the clock code wrote add up to the frequency the firmware thinks it has is to measure
it against something independent.

**Example:** vb135's H7 clock code (PLAT-8) configured PLL1 for a 540 MHz SYSCLK with `D1CPRE` left at its
reset divide-by-2, so the core actually ran at 270 MHz; the bug was only found by timing the DWT cycle counter
against `embassy_time`'s own tick and seeing the ratio come out exactly half of what the prescaler math claimed.
Every CPU-bound path (bulk USB, SD, the WireWeaver dispatcher) had been running at half speed since the last
clock-tree change, unnoticed because nothing else looked wrong.

**Fix:** after any clock-tree change, read `DWT.CYCCNT` across a known number of `embassy_time` ticks (or
`cortex_m::asm::delay` cycles against a scope) once, print the computed core frequency, and compare it to what
the configuration claims. Keep that check in the bring-up notes for the board, not just in the firmware.

**Detection:** check: bench — this only shows up by measuring real hardware; there is no register to read back
that proves the actual core frequency, so it stays a bring-up step, not a lint.

## <a id="clk-2"></a>`CLK-2` — One timer, one owner

embassy-stm32's `time-driver-any` feature lets the HAL pick whichever free timer it likes for the time driver.
That pick is invisible until a peripheral driver (PWM, input capture, the LED's breathe animation) is told to
use the same timer explicitly: then two independent consumers fight over one set of capture/compare registers,
and the symptom is a timer that behaves erratically only on some boards or some builds, because the time
driver's choice depends on what else in the binary claims a timer first.

**Example:** the template's TPL-39 (LED breathe animation on a PWM timer channel) hit exactly this: naming the
time driver `time-driver-any` let embassy pick the LED's own timer for the tick, so the fix is to always name the
time driver explicitly (`time-driver-timN`, never `-any`) and choose N to be a timer nothing else in the firmware
touches — the LED's AF/channel lookup in `stm32-data` excludes the timer picked for the driver before handing it
to `src/led.rs`. The same class of collision shows up wherever a firmware (typec_dongle, the B135 template) adds
a second timer consumer without first checking which timer the time driver already claimed.

**Fix:** name the time driver's timer explicitly in `Cargo.toml` features and keep a single place (the chip
data / board preset) that tracks which timer is taken; a new PWM, capture or one-shot consumer picks from what's
left, never from `-any`'s implicit choice.

**Detection:** check: cfg easy (the `time-driver-any` feature string is a `grep`) + src mid (matching the
driver's configured timer peripheral against the one the time-driver feature names needs the firmware model,
since the timer is usually passed as a generic type parameter, not a string).

## <a id="clk-3"></a>`CLK-3` — Clock security, so a lost oscillator is noticed

An external crystal (HSE, LSE) can stop or glitch in the field (a cracked crystal, a bad solder joint, EMC) and
the core silently keeps running on whatever the clock switches to — or doesn't switch, and locks up — unless the
Clock Security System (CSS) is turned on and its interrupt handler actually does something about it. Without
CSS, a dead HSE on a part that fell back to HSI looks like unrelated flakiness (wrong baud rates, wrong USB
timing) days or weeks after the board left the bench.

**Fix:** enable CSS on HSE (and on LSE where the RTC depends on it) at clock init, and give it a handler that
records the event (so it survives to the next boot's `fw_info`) and takes the safe fallback the application
needs — at minimum, don't let a silently wrong clock keep driving USB or CAN timing.

**Detection:** check: src easy — the CSS enable bit and a bound interrupt handler are both greppable from the
clock-init source without needing the firmware model.

This embedded_bedrock crate's own `ROB-5` tracks CSS as not yet implemented in the firmware template: the
bootstrap enables the oscillators but doesn't yet wire up the CSS handler or the LSE backup-domain reset path,
so a generated firmware that needs this still has to add it by hand until ROB-5 lands.

## <a id="clk-4"></a>`CLK-4` — LSE start-up time off the critical path

A 32.768 kHz crystal (LSE) can take seconds to stabilize, far longer than HSE or an internal RC. Code that waits
for `LSERDY` before doing anything else — boot, USB enumeration, a first response to the host — turns that
start-up time into a visible stall, and on some boards (H7 in particular) the wait can run into several
seconds, long past any host-side timeout that expects a device to answer quickly.

**Fix:** start LSE and move on; let whatever actually needs it (RTC-backed timestamps, calendar features) wait
for `LSERDY` lazily, on first use, instead of gating the whole boot path on it. Boot, USB and the host-facing
API should be available before LSE is ready.

**Detection:** check: review — the wait is fine in isolation, so this is a question of what else runs before it,
which needs reading the boot sequence, not a pattern a lint can flag reliably.

## <a id="clk-5"></a>`CLK-5` — USB needs an exact 48 MHz, every time

USB full-speed timing is unforgiving of clock error, so a part that drives USB from anything other than a
48 MHz clock accurate to the USB spec's tolerance (HSI48 trimmed by CRS, or a crystal-derived 48 MHz) will
enumerate unreliably or not at all, in ways that look like a host or cable problem rather than a clock one. Some
STM32 parts don't have HSI48 or CRS at all, and a generated clock config that leaves "USB clock: TODO" in a
comment is a config that was never actually tried with a USB host.

**Example:** this repo's own firmware template (`TPL-13`) generates a USB clock TODO instead of a working setup
for F411, L476 and F103 — parts without HSI48 / CRS — so a firmware generated for one of those chips with
`--wire-weaver` (USB transport) builds but has never been clocked correctly for USB; the gap is tracked, not
silently shipped.

**Fix:** on a part without HSI48/CRS, derive the exact 48 MHz USB clock from the crystal through the PLL (most
of these parts can), and treat "USB clock: TODO" in generated code as a build that must not reach a board.

**Detection:** check: src mid — matching the configured USB kernel clock source against the part's actual
48 MHz capability (HSI48 trimmed vs. a PLL output) needs the chip's clock-tree model, not just a string match.

## <a id="clk-6"></a>`CLK-6` — Read back the clock you asked for

A kernel clock chosen for a peripheral may not actually divide down to the rate the peripheral needs exactly:
the division is an integer (or near-integer) ratio, and when it doesn't come out exact the peripheral runs at a
slightly wrong rate that the configuration code never notices, because it only *wrote* the divider, it never
*read back* what rate that produced.

**Example:** vb135's FDCAN kernel clock (CAN-1) is 135 MHz feeding 2/4/8 Mbit/s bit rates: the nominal and data
bit-timing registers (NBTP/DBTP) are computed from that 135 MHz assuming it divides exactly, but any error in
the upstream PLL math (ERR/CLK-8) shows up as a bit rate that's off by a fraction of a percent — invisible until
another node on the bus (built from a different, correct, kernel clock) starts seeing bit errors. Typec_dongle's
SBU ADC window (API-14) has the same shape: `OVER8`/`USARTDIV`-style kernel-clock division for a UART or ADC
sequence timing can drop a bit of precision that only shows up as a sequence period slightly off from what was
asked for.

**Fix:** after configuring a kernel clock and its divider for a timing-sensitive peripheral, read back (or
compute from the same inputs the peripheral itself uses) the rate actually achieved, and count a mismatch
against the rate that was asked for instead of trusting the division was exact.

**Detection:** check: test — this needs a real measurement (a bit rate on the bus, a sequence period on the
scope, or at minimum a host-side computation cross-checked against the chip's own achieved-rate register) to
catch a rounding error the configuration code can't see on its own.

## <a id="clk-7"></a>`CLK-7` — Running at the maximum isn't free

A part's maximum clock is a data-sheet number, not a target: running there costs lifetime at small process
nodes, and on parts with ECC-protected tightly-coupled RAM (H7's ITCM/DTCM), ECC itself may only be guaranteed
up to a lower frequency than the core's absolute maximum — so "as fast as the part allows" can silently trade
away a safety feature the firmware was relying on.

**Example:** vb135's H7 core clock (PLAT-8) is capped below the part's absolute maximum specifically because
ITCM/DTCM ECC is only guaranteed up to 520 MHz, not the chip's higher absolute ceiling; running faster would have
meant giving up ECC on code and data RAM to gain a clock speed the application didn't need.

**Fix:** pick the clock from what the application actually needs (measured, not assumed) and check the data
sheet for what each speed grade costs — ECC availability, junction temperature derating, documented lifetime —
before defaulting to the maximum because it's available.

**Detection:** check: review — this is a data-sheet trade-off decision, not something a lint can evaluate
without knowing the application's real throughput need.

## <a id="clk-8"></a>`CLK-8` — Check the fractional PLL did what the math says

A fractional PLL (H7's FRACN field, for instance) can silently fall back to integer mode if the fractional field
is left at zero, and even when it's set, a truncated prescaler downstream can leave a kernel clock running a
fraction of a percent off the intended rate — small enough that embassy's own sanity check at boot (which
refuses an out-of-range core clock) doesn't catch it, but large enough to matter for a time driver or a bus
protocol with tight timing margins.

**Example:** vb135's H7 clock tree (PLAT-9) steers PLL2 (FDCAN kernel clock) and PLL1 (SYSCLK, so the time
driver) through the clock sync; verifying the achieved rate against the requested one there is what caught the
time driver running 0.39 % fast from a truncated prescaler — a drift too small to trip embassy's boot-time range
check but large enough to matter once the firmware is disciplining its clock against a host's UTC.

**Fix:** after programming a fractional PLL, compute the achieved rate from the actual register values written
(FRACN included) rather than from the requested rate, and compare; don't trust that asking for a rate and having
embassy accept it at boot means the rate obtained is the rate intended.

**Detection:** check: test — chip: H7. This needs either a register read-back computation or a real timing
measurement; embassy's boot-time acceptance only rules out grossly invalid configurations, not small fractional
error.
