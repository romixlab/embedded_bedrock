# Firmware review checklist

Condensed from the [check catalogue](checks.md) for a review pass over firmware code (a pull request, a new driver,
an upgrade from the template). Each bracketed ID links to its entry. Items the lint checks (`src`, `cfg`, `elf` in the
catalogue) are listed too: until it covers them, they are checked by hand.

- *Executors:* tasks with a hard deadline on an interrupt executor, not beside the USB server
  [[EXE-1]](checks.md#exe-1), [[DMA-3]](checks.md#dma-3) · its priority set before `start()`, below the interrupts
  that wake it, on an unused vector [[EXE-3]](checks.md#exe-3), [[EXE-4]](checks.md#exe-4), [[EXE-5]](checks.md#exe-5) ·
  shared state behind critical sections [[EXE-6]](checks.md#exe-6) · nothing blocking there or in an interrupt
  [[EXE-2]](checks.md#exe-2) · loops in shared tasks bounded and yielding, the server never waits on a bus
  [[EXE-7]](checks.md#exe-7), [[EXE-8]](checks.md#exe-8), [[EXE-13]](checks.md#exe-13) · no no-op wakers near
  embassy-time [[EXE-9]](checks.md#exe-9) · one executor provider [[EXE-10]](checks.md#exe-10)
- *Interrupts:* only the handled flags cleared, W1C by a write, not `modify()` [[IRQ-1]](checks.md#irq-1),
  [[IRQ-2]](checks.md#irq-2), [[IRQ-3]](checks.md#irq-3) · shared vectors survive a driver's drop
  [[IRQ-4]](checks.md#irq-4) · lines routed are enabled [[IRQ-5]](checks.md#irq-5) · `critical_section`, no nested
  disable / enable [[IRQ-6]](checks.md#irq-6) · interrupts capture, tasks work [[IRQ-7]](checks.md#irq-7)
- *DMA:* buffers in memory the master reaches, cache kept coherent [[DMA-1]](checks.md#dma-1),
  [[DMA-2]](checks.md#dma-2) · overruns counted [[DMA-4]](checks.md#dma-4) · errata that ask for DMA
  [[DMA-6]](checks.md#dma-6)
- *Faults:* HardFault records and resets, no defmt or `cnt!` in fault handlers, `exc_return!` first
  [[FLT-1]](checks.md#flt-1), [[FLT-2]](checks.md#flt-2), [[FLT-3]](checks.md#flt-3) · fault handlers of the core,
  reset cause reported [[FLT-4]](checks.md#flt-4), [[FLT-5]](checks.md#flt-5) · watchdog fed from the main flow only
  [[FLT-7]](checks.md#flt-7) · boot-loop rescue and healthy mark [[FLT-8]](checks.md#flt-8),
  [[FLT-9]](checks.md#flt-9) · critical data and registers checked [[FLT-12]](checks.md#flt-12),
  [[FLT-13]](checks.md#flt-13)
- *Host input:* no panic reachable from a request, nothing that damages the board, unknown values refused,
  timeouts on every wait for the host [[HOST-1]](checks.md#host-1), [[HOST-2]](checks.md#host-2),
  [[HOST-3]](checks.md#host-3), [[HOST-4]](checks.md#host-4), [[HOST-5]](checks.md#host-5) · error branches counted
  [[CNT-1]](checks.md#cnt-1) · logging never blocks [[LOG-1]](checks.md#log-1)
- *Pins:* drivers held for the firmware's life [[PIN-1]](checks.md#pin-1) · safe state first, translators on last
  and off first, direction pins turn when idle [[PIN-2]](checks.md#pin-2), [[PIN-3]](checks.md#pin-3),
  [[PIN-7]](checks.md#pin-7) · one owner per pin, FUNCSEL kept under a peripheral [[PIN-6]](checks.md#pin-6),
  [[PIN-4]](checks.md#pin-4) · pulls set explicitly, unused pins analog [[PIN-5]](checks.md#pin-5),
  [[PIN-8]](checks.md#pin-8) · pin maps from the board record, not features [[PIN-9]](checks.md#pin-9),
  [[PIN-11]](checks.md#pin-11) · lines released on every session end [[PIN-10]](checks.md#pin-10)
- *Memory and flash:* flip-link from `.cargo/config.toml`, stack below `.data` [[STK-1]](checks.md#stk-1),
  [[STK-2]](checks.md#stk-2), [[STK-3]](checks.md#stk-3) · big state built in place [[STK-5]](checks.md#stk-5) ·
  memory.x keeps reserved regions out, backup-register map without overlaps [[MEM-1]](checks.md#mem-1),
  [[MEM-5]](checks.md#mem-5) · torn flash read through the ECC-safe reader, one flash owner
  [[FLS-1]](checks.md#fls-1), [[FLS-2]](checks.md#fls-2)
- *Boot:* entry by jump detected, H7 supply read back, recovery path reachable [[BOOT-1]](checks.md#boot-1),
  [[BOOT-2]](checks.md#boot-2), [[BOOT-4]](checks.md#boot-4) · update wire format versioned
  [[BOOT-8]](checks.md#boot-8) · backup-domain helpers restore what they found [[BOOT-10]](checks.md#boot-10)
- *Chip:* errata of the revision fitted read and quoted at the code [[ERR-1]](checks.md#err-1),
  [[ERR-2]](checks.md#err-2) · the known ones: G0 prefetch across banks, FDCAN, RP2350-E9 [[ERR-3]](checks.md#err-3),
  [[ERR-4]](checks.md#err-4), [[ERR-6]](checks.md#err-6) · time driver on its own timer [[CLK-2]](checks.md#clk-2)
- *Build:* probe pinned, halting runner [[CFG-1]](checks.md#cfg-1), [[CFG-2]](checks.md#cfg-2) · all feature sets
  and both profiles build and fit [[CFG-3]](checks.md#cfg-3), [[SIZ-2]](checks.md#siz-2), [[SIZ-1]](checks.md#siz-1) ·
  no duplicate crates [[CFG-4]](checks.md#cfg-4) · defmt buffer and level explicit [[LOG-2]](checks.md#log-2)
- *Numbers:* size and speed measured at the shipped opt-level, in their own units [[PERF-1]](checks.md#perf-1),
  [[SIZ-4]](checks.md#siz-4) · no 64-bit maths or divides on M0+ hot paths [[PERF-2]](checks.md#perf-2)
