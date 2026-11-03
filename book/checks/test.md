# 🧪 TEST — Tests and bring-up process

Detail for the `TEST` checks of the [check catalogue](../checks.md).

## <a id="test-1"></a>`TEST-1` — a host test that races a free-running tick or sleeps "long enough"

A test that asserts on a value driven by a free-running counter or timer, right after an action that's supposed
to move it, is really asserting "the counter moved within however long my assertion took to run" — which is
true on a quiet CI box and flaky the moment the machine is busy.

**Example:** usb_io_fw's API-51 found `counters_read_reset_and_watch` asserting the tick counter was exactly 0
right after `reset`, while the simulator keeps counting it every 1 ms in the background — about one run in six
failed on timing alone. HOST-6 is the same test caught in the wild: `tests/sim_device.rs` sleeps 20 ms and
expects the tick counter to have moved by then; on a busy PC it failed once, then passed both alone and on two
immediate reruns, which is the signature of a race, not a logic bug.

**Fix:** don't assert an exact value against a free-running clock; read a value, wait for the thing that
actually drives the change (a signal, a channel, a poll with a timeout) instead of a fixed sleep, and assert on
the direction/bound (moved forward, didn't move backward) rather than an exact tick count. API-51's fix set
the counter high before the action and checked it dropped, plus that an unrelated counter stayed exactly 0.

**Detection:** check: review — a lint can flag a bare `sleep`/`Timer::after` immediately followed by an
assertion on a free-running value as a candidate, but telling a legitimate short wait from a race needs a
human to read what the test depends on.

## <a id="test-2"></a>`TEST-2` — a simulator that answers inline hides bugs on the deferred path

A host-side simulator that answers every request synchronously, inline, inside the call that sent it, never
exercises the path a real device takes when an answer comes back later, out of order, or asynchronously from a
different task. A test suite that only runs against that simulator can pass completely while the deferred path
(the one real hardware actually uses) has never been executed once.

**Example:** usb_io_fw's IF-13 shared SD server (`sd_serve`) is the case this check names: a simulator answering
inline would validate the request/response shapes but never the server loop's actual deferred-answer
structure (a credit or grant sent later, from inside the handler, once the SD transfer the request triggered
has actually completed) — the same deferred-answer shape that HOST-7 in this catalogue's `HOST` category warns
about getting wrong.

**Fix:** keep (or add) a simulator path that genuinely defers its answer — a queue drained by a separate step,
not a direct return — and run every test that matters against both the inline and the deferred simulator, not
just whichever one was easiest to write first.

**Detection:** check: review — needs reading whether the test's simulator ever actually defers an answer across
an await point; a lint could flag a simulator implementation with no `.await` or no separate "drain" step as a
candidate for this check.

## <a id="test-3"></a>`TEST-3` — board sequencing rules without host tests of the transition model

A board's safe-sequencing rules (power sources that must never be on together, a translator's enable order, a
"safe" idle state) are exactly the kind of invariant that's easy to state once in a comment and then violate
months later through an innocent-looking change to an adjacent function — unless the rule is checked by a test
that runs on every change, not just read in review.

**Example:** usb_io_fw's `b200_board::chain` module (`src/chain.rs`, part of IF-2's I2C master work — two VREF
sources, two I2C paths, or two SPI translators must never be on together, "not even for a moment") carries its
own `#[cfg(test)] mod tests`: `Chain::SAFE` is asserted safe and VREF-off at the bit level
(`Chain::SAFE.check() == Ok(())`), and the all-zeros state (the plain-clear state of the shift-register chain
at power-up, before any firmware write) is asserted as legal on purpose — a case worth a named test precisely
because the default state is *not* the same as the "safe" state the firmware moves to afterward.

**Fix:** encode a board's enable-order and mutual-exclusion rules as a pure function over the chain/pin state
(a `check()` that returns `Ok`/`Err`, independent of any HAL) and host-test it directly against every state that
matters — the power-up default, the declared-safe state, and the states a legal transition can pass through.

**Detection:** check: test — the lint can only ask whether such a function and its test module exist for a
board that declares mutual-exclusion or ordering rules in its hardware notes; it can't invent the rule itself.

## <a id="test-4"></a>`TEST-4` — a board left running its last test after probe-rs exits

Once probe-rs detaches, the target keeps running whatever the last test flashed it with — including a test
that left a peripheral mid-transaction, a loop spinning, or output pins driven into a state the next test
doesn't expect. A HIL run that resets the target before attaching for a new test gets a clean start every time;
one that doesn't can see the previous test's leftover state read back as if it were the new test's own fault.

**Example:** mcan's Q5a board-to-board HIL rig (two boards talking over real transceivers, not loopback) wrote
down this exact lesson from its own runs: "a board keeps running its last test after probe-rs exits, so reset
before each run" — found because a board's prior test state bled into the next run's result until the harness
was changed to reset both boards before starting the receiver and only then starting the sender
(`hil/run-bus.sh`).

**Fix:** every HIL test harness resets the target (not just re-flashes it) before each test starts, and when
two boards talk to each other, reset both and sequence the start order (receiver ready before sender starts) —
exactly what Q5a's `run-bus.sh` does.

**Detection:** check: review — a HIL harness script is checked for a reset step immediately before each test
run, not only once at the start of a whole suite.

## <a id="test-5"></a>`TEST-5` — two owners of the same probe

A board attached to two debug sessions at once — another session's `probe-rs`, a stray `gdb`, an IDE's
background attach — can produce faults that look exactly like a firmware bug: register pokes over SWD/JTAG stall
the core, land mid-instruction, or race a DMA transfer that was never meant to share the bus with a debugger.
The firmware did nothing wrong; it was just sharing hardware access with something that doesn't know the
firmware is running.

**Example:** typec_dongle's FW-44 bring-up ran 227 s clean with no panic on a B153A over J-Link — but only once
"another session's concurrent probe-rs/gdb use of the same board" (RTC_CR pokes from a different Claude session's
bootloader test) stopped; while it was running, the interference even left the hub's USB enumeration wedged on
the host side. FW-48 is the sharper version: once in five RTC_CR pokes over J-Link stalled the core long enough
to cause ADC DMA overruns, which 130 ms later triggered an unhandled interrupt into `DefaultHandler`, whose
`cnt!`/`error!` then panicked inside defmt's nested-logging path (`default_panic`) — a real firmware bug
(logging inside a fault/default handler, see `FLT-2` in this catalogue) but *triggered* only by a second,
unrelated debug session poking the same chip.

**Fix:** one probe, one owner, for the duration of a test or a bring-up session; before blaming the firmware
for a fault seen only under a debugger, check `probe-rs list`/process list for a second attached session on the
same board.

**Detection:** check: review — nothing in the ELF or source says another session was attached; this is a
bring-up-process discipline, checked by asking "was anything else touching this board" before filing a bug.

## <a id="test-6"></a>`TEST-6` — a timing- or layout-dependent fault bisected before the errata were checked

A fault that only reproduces at a certain clock rate, flash layout, or code size looks like exactly the kind of
thing `git bisect` is built for — except when the actual cause is a documented silicon erratum that depends on
those same variables (a prefetch failure triggered by *where* a branch happens to land relative to a flash
bank boundary, for instance), bisecting burns real time rediscovering what the errata sheet already states for
free.

**Example:** typec_dongle's FW-38 HardFault (ES0548 2.2.10, flash prefetch across G0B1's dual banks) is the case
in point: the fault only appeared once prefetch was turned on (FW-24) and only under specific code-size/layout
conditions, which is exactly the signature a naive bisect would chase as "some unrelated commit changed the
binary's layout" — instead of recognizing the erratum's own description ("the code prefetch may fail upon
branching and function calls across flash memory banks, when the device operates in dual-bank mode", status N
on every revision of that part) as the direct explanation. `~/git/tpm/roles/firmware.md` states the general
rule this example backs: check the errata sheet for the chip and revision in use before spending bisection time
on a timing- or layout-sensitive fault.

**Fix:** when a fault's reproducibility depends on clock rate, code layout, or timing rather than clear logic,
read the errata sheet for the exact chip and silicon revision before bisecting; a matching erratum turns a
multi-hour bisect into a known workaround.

**Detection:** check: review — this is a process check (did the errata sheet get read first), not something an
automated tool can verify after the fact; `ERR-1` in this catalogue is the standing rule it feeds.

## <a id="test-7"></a>`TEST-7` — a chip blamed before our own side was measured

An external chip is an easy first suspect for a communication fault — especially a chip from a vendor whose
datasheet is thinner than ours, or one that's already had other problems — but blaming it without measuring our
own side first means the real cause (our own timing, our own task priority) can sit unfixed while a chip that
was innocent the whole time gets worked around or replaced.

**Example:** typec_dongle's FW-62 is this check's source and a worked example of taking the blame back. USB-PD
GoodCRC retries against a sink were initially suspicious of the sink's FUSB302B, but the actual investigation
(`~/git/tpm/prj/P2424-report/design/pd-controller-plan.md`) ruled it out methodically: (1) the sink's own log
showed every retried message's `MessageID` received twice, meaning the sink had the first attempt whole — it
wasn't losing anything; (2) Donguru's PD task shared the thread-mode executor with its USB server, and embassy
arms UCPD's RX DMA only inside `receive()`, so a USB server turn across the end of a transmission let the
GoodCRC's first bytes overrun RXDR; (3) retries ran 3.5% of Get_Sink_Cap exchanges with logging streaming, 0.5%
without — correlated with *our* logging load, not the sink; (4) moving extra I2C reads onto the sink to rule out
load-on-the-sink as a cause changed nothing (3.7-4.6% either way, no better or worse than the 3.5% baseline);
(5) the actual fix — running the PD task on an interrupt executor at priority 1 — took retries to 0 in 2000
exchanges. The sink chip never needed a workaround.

**Fix:** before writing up or working around an external chip's apparent fault, measure what our own firmware
was doing at the moment of the fault (task scheduling, DMA arming windows, our own load) with the same rigor
that would be demanded of the external chip; a chip that NAKs or retries while *we* were late is not the chip's
bug.

**Detection:** check: review — needs the same kind of correlated measurement FW-62 used (our own counters and
logs lined up against the failure), which a lint has no way to construct; this check exists to remind a reviewer
to ask for that measurement before accepting "the chip is at fault" as a conclusion.
