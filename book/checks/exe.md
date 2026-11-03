# EXE checks, explained

Detail page for the `EXE` category of the [check catalogue](../checks.md#-exe---executors-and-tasks). Each section
explains one check: what it catches, why the firmware fails without it, a concrete example, how it is checked, and
the fix. The one-line entries in the catalogue stay the source of truth for ID, severity and wording; this page is
the commentary. Executors and tasks are embassy concepts throughout: a thread executor (`#[embassy_executor::main]`)
runs every spawned task in thread mode, cooperatively; an interrupt executor (`InterruptExecutor`) runs its tasks
inside an interrupt handler, so it preempts thread mode but nothing of equal or higher priority.

## `EXE-1` A hard-deadline task sharing the thread executor with bulk work <a id="exe-1"></a>

🛑 error · check: src mid · [catalogue entry](../checks.md#exe-1)

**What it catches.** A task whose correctness depends on running within a short, hard window after an event (a PHY
that only arms its RX DMA inside `receive().await`, a software-computed CRC that must answer before the next
symbol, a DMA ring consumer that must drain before the ring wraps) spawned on the same thread executor as tasks with
no such deadline (a USB control server, a logger).

**Why it matters.** The thread executor runs tasks cooperatively: once any task is polled, every other task on that
executor waits until it yields or awaits. A long-running or bursty neighbour (a USB enumeration storm, a log flush)
delays the deadline task by however long the neighbour keeps the executor, and the delay is irregular and only
shows up under load.

**Example.** `pd_task` on typec_dongle shared the thread executor with the WireWeaver USB server; embassy's UCPD
driver only re-arms its RX DMA inside `receive().await`, so a USB burst that kept the server running past the next
GoodCRC window overran RXDR, seen as a 3.5 % retry rate.

**How to check.** From the firmware model (tasks, their executor, spawn order), flag a task whose doc comment or
name suggests a hard answer deadline that shares an executor with a task doing bulk or unbounded work.

**Fix.** Move the deadline task to its own `InterruptExecutor` on an unused, correctly prioritised vector
([`EXE-3`](#exe-3), [`EXE-4`](#exe-4), [`EXE-5`](#exe-5)) — on typec_dongle, `TIM7_LPTIM2` at priority 1.

## `EXE-2` Blocking call on an interrupt executor or inside an interrupt handler <a id="exe-2"></a>

🛑 error · check: src hard · [catalogue entry](../checks.md#exe-2)

**What it catches.** `block_for`, a HAL's `blocking_*` call, `nb::block!`, a busy-wait loop, or
`embassy_futures::block_on` reached from a task running on an `InterruptExecutor`, or from an interrupt handler
directly.

**Why it matters.** An interrupt executor already runs inside an interrupt context: blocking there stalls every
interrupt at the same priority and below for however long the call waits, including the ones the blocked code
itself may be waiting on (a deadlock if the wait depends on an interrupt that can't fire). The failure is a hang,
not a wrong answer, which makes it expensive to isolate after the fact.

**Example.** A HAL `blocking_write` call left behind while moving a driver from the thread executor to an interrupt
executor (see `EXE-11`) stalls every lower-priority interrupt until the transfer finishes.

**How to check.** Needs data-flow reachability from a task/handler's body to a known-blocking call (`nb::block!`,
HAL methods named `blocking_*`, `block_for`, `embassy_futures::block_on`) — more than a syntactic match, hence
`hard`.

**Fix.** Use the `async` driver call and `.await` it; if the peripheral has no async path, move the work back to
the thread executor or to a task that can tolerate blocking.

## `EXE-3` Interrupt executor started before its priority is set <a id="exe-3"></a>

🛑 error · check: src easy · [catalogue entry](../checks.md#exe-3)

**What it catches.** `InterruptExecutor::start()` called before the vector's priority is lowered from its default
(the NVIC reset default is the *highest* priority, 0 on most Cortex-M parts).

**Why it matters.** `start()` unmasks the interrupt right away. Until the priority is set it runs at priority 0,
above every other interrupt in the system including the ones the new executor is meant to sit below — exactly the
inversion `EXE-4` exists to prevent, just for the short window between `start()` and the priority write.

**Example.** usb_io_fw's executor set-up code (see AGENTS.md "executors") sets the priority with
`cortex_m::peripheral::NVIC::set_priority` *before* calling `start()`, the order every new interrupt executor in
the codebase now follows.

**How to check.** A pattern match on the firmware model: for each `InterruptExecutor`, its `set_priority` call
(or the HAL equivalent) must precede its `start()` call in source order.

**Fix.** Set the priority first, then call `start()`.

## `EXE-4` Interrupt executor priority not below what it must preempt <a id="exe-4"></a>

⚠️ warning · check: src mid · [catalogue entry](../checks.md#exe-4)

**What it catches.** An interrupt executor whose NVIC priority is not numerically below (i.e. does not have a
*higher* preemption priority than) the peripheral, DMA and time-driver interrupts that wake the tasks running on
it.

**Why it matters.** An interrupt executor exists to preempt thread mode without waiting for the scheduler; if its
own priority is at or below the interrupts that feed it data, those interrupts can't run while it executes, and the
data the executor's task expects to see hasn't arrived yet — same shape of bug as running on the thread executor,
just one layer deeper.

**Example.** typec_dongle's PD task executor sits at priority 1, below USB/UCPD/DMA at priority 0 (`EXE-1`); vb135's
CAN handling executor sits at priority 7, below FDCAN at priority 6 — "below" here meaning the lower number, higher
NVIC priority.

**How to check.** From the firmware model: for each interrupt executor, the set of interrupts whose handlers wake
one of its tasks (via a signal, channel send, or waker); each such interrupt's configured priority must be
numerically higher (lower urgency) than the executor's.

**Fix.** Re-number priorities so the executor sits strictly above everything that must preempt *it*, and strictly
below the things it must not block.

## `EXE-5` Interrupt executor sharing a vector with a bound driver or used peripheral <a id="exe-5"></a>

🛑 error · check: src mid · [catalogue entry](../checks.md#exe-5)

**What it catches.** An `InterruptExecutor` placed on an NVIC vector that `bind_interrupts!` also routes to a
driver, or that a used peripheral can raise on its own.

**Why it matters.** The vector table has one handler per vector. embassy's `bind_interrupts!` macro generates the
real handler for a bound vector; an executor "started" on the same vector either fails to build or silently starves
whichever of the two the generated code doesn't reach, which shows up as a peripheral that stops responding under
load, not at start-up.

**Example.** usb_io_fw uses the unused `SWI_IRQ_0` software interrupt for its executor; vb135 uses `SWPMI1`, a
peripheral not fitted on that board — both picked because nothing else claims the vector.

**How to check.** From the firmware model: the executor's chosen vector must not appear in any `bind_interrupts!`
table, and must not belong to a peripheral the chip-data model marks as enabled/used.

**Fix.** Pick a vector of a peripheral that is genuinely unused on this chip and board, and say so in a comment next
to the `InterruptExecutor` declaration (so the next edit that enables that peripheral doesn't collide silently).

## `EXE-6` Interrupt-executor state shared with thread mode behind the wrong mutex <a id="exe-6"></a>

🛑 error · check: src mid · [catalogue entry](../checks.md#exe-6)

**What it catches.** State written from a task on an interrupt executor and read (or written) from thread-mode
code, guarded by `ThreadModeRawMutex`, `NoopRawMutex`, or a bare `static mut` instead of `CriticalSectionRawMutex`.

**Why it matters.** `ThreadModeRawMutex` and `NoopRawMutex` assume nothing but thread mode ever touches the data;
an interrupt executor can preempt thread mode mid-access, so the "lock" does nothing against it and the access
races — a `static mut` races the same way with no lock at all. The result is a value that's occasionally torn or
stale, reproducing only under the timing that lets the interrupt land mid-update.

**Example.** typec_dongle's PD state, once a task moved to an interrupt executor, needed its mutex changed to
`CriticalSectionRawMutex` to stay safe against the thread-mode USB server reading the same state (FW-62 note).

**How to check.** From the firmware model: for each static/mutex shared by a task on an interrupt executor and any
thread-mode code path, its mutex type must be `CriticalSectionRawMutex` (or the access must go through a channel).

**Fix.** Switch the mutex to `CriticalSectionRawMutex`, or move the shared state behind a channel/signal instead of
a shared cell.

## `EXE-7` Unbounded drain loop in a shared task <a id="exe-7"></a>

⚠️ warning · check: src mid · [catalogue entry](../checks.md#exe-7)

**What it catches.** A loop in a task on a shared (thread) executor that keeps going "until a queue is empty" or
logs every iteration of a batch, with no bound on how many iterations it can take and no `yield_now()`/await
between them.

**Why it matters.** Below the input rate the loop behaves; above it, it never reaches its own exit condition before
more work arrives, and since the thread executor is cooperative, every other task on it starves for as long as the
loop runs — not a crash, just everything else on that executor going quiet.

**Example.** typec_dongle's `uart_pump` task (FW-42) and `log_flood` (FW-27, 62 ms in one call) both drained their
queues this way; the fix bounds the number of items taken per poll and yields in between.

**How to check.** From the firmware model (`mid`): a loop with no static bound on iteration count and no await
inside its body, inside a task that also isn't the sole task on its executor.

**Fix.** Cap how much one poll drains (a fixed batch size) and `.await` a yield between batches so other tasks get
a turn.

## `EXE-8` Request server blocking on a bus or slow peripheral inside a handler <a id="exe-8"></a>

⚠️ warning · check: src mid · [catalogue entry](../checks.md#exe-8)

**What it catches.** A WireWeaver (or similar) request-handler function that itself awaits a bus transfer (I²C,
SPI, a slow external chip) instead of handing the request to the task that owns the bus and answering once that
task reports back.

**Why it matters.** The server loop processes one request at a time; a handler that blocks on a bus transfer stalls
every other request on the same server for the transfer's duration, including requests that have nothing to do
with that bus.

**Example.** usb_io_fw's I²C handlers originally ran their transfers inline in the server loop (IF-2); every I²C
request serialised the whole device API behind the bus's own timing.

**How to check.** From the firmware model: a request handler body that awaits a bus driver call directly, rather
than sending to a channel owned by a bus task and awaiting the reply channel.

**Fix.** Give the bus its own task; the handler sends the request over a channel and awaits the answer, so other
handlers keep running while the transfer is in flight.

## `EXE-9` Future polled with a no-op waker that can reach embassy-time <a id="exe-9"></a>

🛑 error · check: src mid · [catalogue entry](../checks.md#exe-9)

**What it catches.** `poll_once`, a hand-rolled no-op waker, or any manual poll of a future whose call graph can
reach `embassy_time` (a timer, `Timer::after`, a time-driver-backed channel).

**Why it matters.** embassy-time's timer queue registers the *real* waker it was polled with and panics
("Found waker not created by the Embassy executor") the moment it's polled with one that isn't — not a silent bug,
but one that only fires the first time that code path actually needs the timer, which can be well after the
feature shipped.

**Example.** wire_weaver's TR-13: every RTT debug-tool device got reset on its first request, because the first
request's handling path reached an embassy-time wait while being polled with a no-op waker used for an initial
check.

**How to check.** From the firmware model (`mid`): trace from every manual `poll_once`/no-op-waker poll site to
whether its future's call graph can reach an embassy-time primitive.

**Fix.** Don't poll manually across code that can wait on embassy-time; if an immediate "is it ready" check is
needed, use a non-blocking accessor instead of polling the real future once.

## `EXE-10` Two crates both providing the executor and `__pender` <a id="exe-10"></a>

🛑 error · check: cfg easy · [catalogue entry](../checks.md#exe-10)

**What it catches.** `embassy-executor`'s `platform-cortex-m` feature enabled together with an `executor-*` feature
of `embassy-rp` or `embassy-stm32` (or any combination that makes two crates define the executor's `__pender`
symbol).

**Why it matters.** Only one crate may define `__pender`; with two, the link either fails outright or (worse) picks
one silently depending on link order, so the executor that actually runs may not be the one the code was written
against.

**Example.** Both usb_io_fw's AGENTS.md and the template's `frameworks.md` call this out explicitly, because the
chip HAL crates offer their own `executor-*` feature as a convenience and it's easy to turn on by habit next to
`platform-cortex-m`.

**How to check.** `cfg easy`: read the resolved feature set of `Cargo.toml` / `Cargo.lock` and flag the combination
directly — no firmware model needed.

**Fix.** Pick one source for the executor (usually the chip HAL's `executor-*` feature on RP and STM32 targets) and
drop `platform-cortex-m`, or vice versa.

## `EXE-11` Task moved to an interrupt executor without a new stack look <a id="exe-11"></a>

⚠️ warning · check: elf hard · [catalogue entry](../checks.md#exe-11)

**What it catches.** A task re-spawned on an `InterruptExecutor` whose worst-case stack depth was only ever
measured (or budgeted) for running on the thread executor.

**Why it matters.** An interrupt executor's tasks run on top of whatever thread-mode stack usage happens to be in
effect when the interrupt lands — the deepest point of the deepest thread-mode call chain, not a fresh stack. A task
that fit comfortably on its own now stacks on top of that worst case, and the margin that `STK-6` budgets for can
disappear without any code in the task itself changing.

**Example.** typec_dongle's FW-62 note: moving `pd_task` to an interrupt executor meant re-checking what the
deepest thread-mode call chain already used, not just the task's own frame.

**How to check.** `elf hard`: needs the linked ELF's call graph for both the deepest thread-mode path and the
task's own worst case, summed — the kind of analysis `STK-6` already needs.

**Fix.** Re-run the stack-depth analysis with the task's frames stacked on top of the thread-mode worst case, not
in isolation, and raise the stack budget if it no longer fits.

## `EXE-12` `async fn` that never awaits <a id="exe-12"></a>

💡 suggestion · check: src easy · [catalogue entry](../checks.md#exe-12)

**What it catches.** A function marked `async fn` whose body never reaches an `.await` — common in generated
request-handler dispatch tables where most handlers are simple and synchronous but the trait requires `async fn`.

**Why it matters.** Every `async fn`, awaiting or not, becomes a state machine in the embassy dispatcher future and
costs flash for its state and poll logic; on a flash-constrained part that adds up across a large handler table even
though none of these functions ever actually suspend.

**Example.** A fw-size measurement (P2534) found 115 of 127 generated handlers never awaited, and making them plain
synchronous functions (called from the one `async fn` that does the dispatch) saved 17.4 KiB at `opt-level = "s"`.

**How to check.** `src easy`: syntactically walk each `async fn` body for the presence of any `.await` expression;
flag the ones with none.

**Fix.** Make the handler a plain `fn` and call it synchronously from the dispatcher; only the dispatcher itself
needs to be `async`.

## `EXE-13` Ring-buffered UART reader looping on 1-byte reads <a id="exe-13"></a>

⚠️ warning · check: src mid · [catalogue entry](../checks.md#exe-13)

**What it catches.** A loop around `RingBufferedUartRx::read` that doesn't batch: on a busy line it gets short
reads (as little as 1 byte) back-to-back with no wait in between.

**Why it matters.** Each `read` call that returns less than requested still costs a full round through the async
machinery; at high baud rates this turns into effectively 100 % CPU spent re-polling for single bytes instead of
letting the ring fill.

**Example.** typec_dongle's `RX_GATHER` fix (FW-42) at 921600 Bd: waiting about 1 ms after a short read before
calling again let the ring accumulate a useful chunk instead of being drained byte by byte.

**How to check.** `src mid`: a loop calling `RingBufferedUartRx::read` with no delay/yield between a short read and
the next call.

**Fix.** After a short read, wait a short, fixed time (around 1 ms, tuned to the baud rate) before reading again, so
the ring has something to gather.

## See also

- [Review checklist](../review-checklist.md) — *Executors* condenses EXE-1, EXE-3 through EXE-10 and EXE-13 into one
  pass over a new driver or task.
- [Bring-up checklist](../bringup-checklist.md) step 7 (*Load*) exercises EXE-1 against real host traffic.
- `usb_io_fw` AGENTS.md "executors" for the priority-then-start convention behind `EXE-3`.
