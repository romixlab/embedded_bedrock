# Firmware check catalogue

What a review of bare-metal Rust firmware (embassy, HAL crates, bare) should catch: every lesson we paid for on a
board, plus the generic firmware checklist, by ID and severity. It is to firmware what better_erc's catalogue is to
schematics. Status of the code that automates a check lives in [FEATURES.md](../FEATURES.md) (area `LINT`); the
condensed [review checklist](review-checklist.md) and [bring-up checklist](bringup-checklist.md) link back here by ID.

- **IDs** (`EXE-1`, `DMA-3`) are stable: never renumber or reuse one. A new check takes the next free number of its
  category. The category prefixes never reuse a FEATURES.md area (`TPL`, `DEV`, `BLD`, `ROB`, `CLI`, `TST`, `DOC`,
  `MAP`, `LINT`).
- **One check, one concern**, so a finding, a waiver, a test fixture and a line of code each map to one ID. An entry
  that grows a second concern is split: the old ID keeps its first concern, the others get new numbers.
- **Severity** of a finding: 🛑 error (the firmware misbehaves, hangs, bricks or damages hardware) · ⚠️ warning (a
  risk or a lost margin, depends on the board) · 💡 suggestion (size, speed, debuggability, conventions).
- **Where it is checked** (`check:` on each line):
  - `src`: a lint on the firmware's Rust source (syn, no type information);
  - `cfg`: a check on `Cargo.toml`, `.cargo/config.toml`, `memory.x`, `build.rs`, `bedrock_fw.json`, the justfile;
  - `elf`: a check on the linked ELF (symbols, sections, stack sizes, disassembly);
  - `test`: a host or on-target test the firmware should have; the lint can only ask for it;
  - `review`: a human (or an agent) reading the code or the datasheet; the lint may list candidates;
  - `bench`: on hardware, at bring-up or after a change (scope, probe, power cuts).
  The tier after it (`easy`, `mid`, `hard`) says how hard the automatic check is: `easy` a pattern or a value,
  `mid` needs the firmware model (tasks, executors, interrupts, pins) or chip data, `hard` needs data flow or a call
  graph. A `chip:` note limits a check to some parts.
- **Sources** sit in a comment at the end of each line: the repo and FEATURES.md id of the bug it came from
  (`typec_dongle FW-62`), or `notion` for the generic firmware checklist imported from Notion on 3 Oct 2026
  (tpm `roles/checklists/firmware.md`). A lesson found on a board becomes a new entry here with its source, so the
  list grows from real failures.
- **Detail pages** under [checks/](checks/), one per category, explain each check: what goes wrong, why, an example,
  the fix, how it is detected, references (errata quoted from the document). EXE, IRQ, DMA, STK, MEM, SIZ, FLT, BOOT,
  PWR, FLS, LOG and CNT are written (LINT-7); PIN, CLK, ERR, EXT, USB, HOST, CFG, PERF and TEST are still planned
  (LINT-8).

## Concurrency

### ⏱️ EXE - Executors and tasks
- <a id="exe-1"></a>🛑 `EXE-1` A task with a hard answer deadline (a PHY that arms RX DMA only inside `receive().await`, a software GoodCRC, a DMA ring consumer) runs on the thread executor beside bulk work (USB server, logging) · check: src mid · <!-- typec_dongle FW-62: embassy UCPD arms RX DMA only in receive(); pd_task beside the WW USB server overran RXDR on GoodCRC, 3.5 % retries; fix: InterruptExecutor on TIM7_LPTIM2 at P1 -->
- <a id="exe-2"></a>🛑 `EXE-2` Blocking call (`block_for`, `blocking_*` HAL calls, `nb::block!`, busy-wait loops, `embassy_futures::block_on`) in a task on an interrupt executor or in an interrupt handler · check: src hard
- <a id="exe-3"></a>🛑 `EXE-3` Interrupt executor started before its IRQ priority is set: `start()` unmasks the IRQ at the default (highest) priority · check: src easy · <!-- usb_io_fw AGENTS.md executors -->
- <a id="exe-4"></a>⚠️ `EXE-4` Interrupt executor priority not below the peripheral, DMA and time-driver interrupts that wake its tasks (it must preempt thread mode, never them) · check: src mid · <!-- typec_dongle FW-62 (P1 under USB/UCPD/DMA at P0); vb135 can.rs (P7 under FDCAN at P6) -->
- <a id="exe-5"></a>🛑 `EXE-5` Interrupt executor on a vector that a driver also binds (`bind_interrupts!`) or that a used peripheral raises: use the vector of a peripheral that is unused, and say so in a comment · check: src mid · <!-- usb_io_fw SWI_IRQ_0; vb135 SWPMI1 -->
- <a id="exe-6"></a>🛑 `EXE-6` State shared between a task on an interrupt executor and thread-mode code behind `ThreadModeRawMutex` / `NoopRawMutex` or a `static mut`: use `CriticalSectionRawMutex` · check: src mid · <!-- typec_dongle FW-62 note -->
- <a id="exe-7"></a>⚠️ `EXE-7` Loop in a shared task that works until a queue is empty (or logs in a loop) without a bound and a yield: above the input rate it never ends and starves the executor · check: src mid · <!-- typec_dongle FW-42 uart_pump; FW-27 log_flood (62 ms in one go) -->
- <a id="exe-8"></a>⚠️ `EXE-8` Request server waits on a bus or a slow peripheral inside a handler: defer the transfer to the task that owns the bus and answer from the server loop · check: src mid · <!-- usb_io_fw IF-2 (I2C handlers ran transfers inline) -->
- <a id="exe-9"></a>🛑 `EXE-9` Future polled with a no-op waker (`poll_once`, `noop_waker`) that can reach `embassy_time`: embassy-time panics "Found waker not created by the Embassy executor" · check: src mid · <!-- wire_weaver TR-13: every RTT device reset on its first request -->
- <a id="exe-10"></a>🛑 `EXE-10` Two crates provide the executor and `__pender` (embassy-rp or embassy-stm32 `executor-*` features and embassy-executor `platform-cortex-m` together) · check: cfg easy · <!-- usb_io_fw AGENTS.md; template frameworks.md -->
- <a id="exe-11"></a>⚠️ `EXE-11` Task moved to an interrupt executor without a new look at the stack: it runs on top of the deepest thread-mode stack · check: elf hard · <!-- typec_dongle FW-62 note -->
- <a id="exe-12"></a>💡 `EXE-12` `async fn` that never awaits (handlers especially): each one is a state in the dispatcher future and costs flash · check: src easy · <!-- P2534 fw-size: 115 of 127 handlers, -17.4 KiB at z -->
- <a id="exe-13"></a>⚠️ `EXE-13` Ring-buffered reader (`RingBufferedUartRx::read`) in a loop without batching: a busy line gives 1-byte reads and 100 % CPU; wait about 1 ms after a short read · check: src mid · <!-- typec_dongle FW-42 RX_GATHER (921600 Bd) -->

### ⚡ IRQ - Interrupts and critical sections
- <a id="irq-1"></a>🛑 `IRQ-1` Interrupt flags cleared wholesale (all-ones or the read value written back to a W1C register): an event between the read and the clear is lost; clear exactly the flags handled · check: src mid · <!-- mcan I1/Y6 (RFxL lost); notion -->
- <a id="irq-2"></a>🛑 `IRQ-2` `rc_w1` / W1C flags cleared with `modify()` (read-modify-write) instead of a write of the one bit: it clears flags that were set meanwhile · check: src mid (SVD access types) · <!-- embassy-stm32 0.6 G0 ADC ISR wiped CCRDY (tpm log Oct 2026) -->
- <a id="irq-3"></a>⚠️ `IRQ-3` Handler that doesn't check its source flag before acting, or clears it late (double interrupt) · check: review · <!-- notion -->
- <a id="irq-4"></a>🛑 `IRQ-4` Several drivers on one shared NVIC vector, and a driver that disables "its" interrupt on drop: the others go deaf (G0B1 `USB_UCPD1_2`) · check: src mid, chip table · <!-- typec_dongle API-30 -->
- <a id="irq-5"></a>⚠️ `IRQ-5` Interrupt line routed (ILS) but not enabled (ILE), or enabled without a handler bound · check: src mid · <!-- mcan I2 -->
- <a id="irq-6"></a>⚠️ `IRQ-6` Nested disable / enable of interrupts by hand: an inner enable turns them on under an outer section; use `critical_section::with` · check: src easy · <!-- notion -->
- <a id="irq-7"></a>💡 `IRQ-7` Interrupt does more than capture (formatting, encoding, logging): a few stores and a counter, the rest in task context · check: elf mid · <!-- wire_weaver AGENTS.md -->
- <a id="irq-8"></a>⚠️ `IRQ-8` Unused interrupt vectors not caught: `DefaultHandler` must count the IRQ number (and not log) · check: src easy · <!-- notion; typec_dongle FW-48 -->

### 🔁 DMA - DMA and caches
- <a id="dma-1"></a>🛑 `DMA-1` DMA buffer in memory the DMA master can't reach (H7 DTCM for IDMA, SDMMC, USB OTG; flash for some masters): it hangs or reads zeros · check: elf mid, chip table · <!-- vb135 PLAT-5; notion -->
- <a id="dma-2"></a>🛑 `DMA-2` D-cache on with DMA buffers in cacheable RAM and no clean / invalidate or MPU non-cacheable region · check: src mid · chip: M7 · <!-- vb135 PLAT-5; template stm32-notes -->
- <a id="dma-3"></a>⚠️ `DMA-3` Receive DMA armed only inside an await (embassy UCPD `receive()`, one-shot UART reads): data that arrives between two awaits overruns; keep the task on time (EXE-1) or use a ring buffer · check: review · <!-- typec_dongle FW-62 -->
- <a id="dma-4"></a>⚠️ `DMA-4` DMA ring overrun not counted: every ring consumer counts overruns with `cnt!` · check: src mid · <!-- typec_dongle FW-26 -->
- <a id="dma-5"></a>💡 `DMA-5` A per-element `%` in the ring copy on a core without a divider (embassy `ReadableDmaRingBuffer` per-byte modulo: 100 % CPU at 580 kB/s on G0) · check: elf mid · chip: M0+ · <!-- P2534 upstream-embassy-dma-ringbuffer-copy -->
- <a id="dma-6"></a>⚠️ `DMA-6` Erratum asks for DMA where the code polls (G0 UCPD TX underflow then TXHRST corrupts the next CRC: ES0548 2.17.1) · check: src mid, chip table · <!-- typec_dongle pd.rs -->
- <a id="dma-7"></a>💡 `DMA-7` "Things just hang": suspect DMA into a wrong memory region first · check: review · <!-- notion -->

## Memory

### 🧱 STK - Stack
- <a id="stk-1"></a>🛑 `STK-1` flip-link not the linker: `linker = "flip-link"` under `[target.<triple>]` in `.cargo/config.toml` · check: cfg easy · <!-- embedded_bedrock ROB-12; typec_dongle FW-1; usb_io_fw PLAT-1 -->
- <a id="stk-2"></a>🛑 `STK-2` `cargo:rustc-linker=` (or another unknown `cargo:` key) printed by build.rs: not a build-script directive, Cargo ignores it silently · check: cfg easy · <!-- embedded_bedrock ROB-12 -->
- <a id="stk-3"></a>🛑 `STK-3` Stack not below `.data` / `.bss` in the ELF (`_stack_start` != start of `.data`), or a zero-size stack · check: elf easy · <!-- embedded_bedrock ROB-12; flip-link 0.1.12 empty .sram8 -->
- <a id="stk-4"></a>🛑 `STK-4` cortex-m-rt `paint-stack` on before its BusFault is understood · check: cfg easy · <!-- usb_io_fw a01319a, embedded_bedrock ROB-6 -->
- <a id="stk-5"></a>⚠️ `STK-5` Big state built by value and moved into a static (`StaticCell::init(Big::new())`): the frame holds it whole; build it in place (`const fn new`, init into `MaybeUninit`) · check: src mid + elf mid · <!-- wire_weaver ObserveBatches: 31.6 KB frame on M4F -->
- <a id="stk-6"></a>⚠️ `STK-6` Stack frame over a budget (per function) or a worst-case stack depth over the stack region · check: elf hard · <!-- firmware pack "stack, cycles and flash" -->
- <a id="stk-7"></a>🛑 `STK-7` Stack overflow never provoked on hardware after a RAM layout change: it must end in a fault, not in corrupted `.data` · check: bench · <!-- embedded_bedrock ROB-1 -->

### 🗺️ MEM - Memory layout and linker
- <a id="mem-1"></a>🛑 `MEM-1` `FLASH` length in memory.x covers a region kept for something else (settings, bootloader state, DFU): the image can grow into it · check: cfg mid · <!-- usb_io_fw PLAT-13/15 -->
- <a id="mem-2"></a>⚠️ `MEM-2` Extra RAM section with a tail `. = ALIGN(n)` or a symbol in it: lld keeps an empty section that flip-link counts as used · check: cfg easy · <!-- embedded_bedrock ROB-12 (RP2350 .sram8) -->
- <a id="mem-3"></a>🛑 `MEM-3` RAM init or zeroing range overlaps `.uninit` (crash record, handoff data) · check: elf mid · <!-- template 0.4 bedrock_crash vs usb_io_fw init_ram.rs -->
- <a id="mem-4"></a>🛑 `MEM-4` Extra SRAM bank used before its RCC clock enable · check: src mid · <!-- embedded_bedrock TPL-3; notion -->
- <a id="mem-5"></a>🛑 `MEM-5` Backup-register map overlaps: `bkp_cnt!` buffer, boot word, boot reason and board state on the same registers (`CNT_BKP_BUFFER_SIZE_WORDS` past the first fixed one) · check: cfg mid + elf mid · <!-- vb135 BOOT-5/7; template 0.4.1 -->
- <a id="mem-6"></a>⚠️ `MEM-6` `.cnt_bkp_buffer` not at the backup-register address · check: elf easy · <!-- template 0.3.0 -->
- <a id="mem-7"></a>⚠️ `MEM-7` Flash partition symbols for embassy-boot written as absolute addresses where offsets are expected (`- ORIGIN(FLASH)`) · check: cfg easy · <!-- template memory-layout.md -->
- <a id="mem-8"></a>⚠️ `MEM-8` Settings store smaller than two erase pages, or on the code's bank of a dual-bank part (an erase stalls the code) · check: cfg mid, chip table · <!-- template memory-layout.md; donguru_template -->
- <a id="mem-9"></a>🛑 `MEM-9` Data that must survive the ROM bootloader kept in RAM the ROM uses (H7: AXI SRAM from 0x24000000, DTCM from 0x20004100): keep it in backup registers · check: elf mid, chip table · <!-- vb135 BOOT-4 -->
- <a id="mem-10"></a>⚠️ `MEM-10` RWX LOAD segment or a writable symbol in `.rodata` (`static mut` in a read-only section) · check: elf easy · <!-- cnt _CNT_SIGNATURE -->
- <a id="mem-11"></a>⚠️ `MEM-11` Linker warnings ignored (section alignment, `.text` not 8-aligned after `.start_block` on RP235x) · check: cfg easy · <!-- template 0.4.3 -->
- <a id="mem-12"></a>🛑 `MEM-12` defmt in the dependencies without `-Tdefmt.x` (no `.defmt` section) · check: cfg easy + elf easy · <!-- book/issues.md -->

### 📦 SIZ - Flash and RAM budget
- <a id="siz-1"></a>⚠️ `SIZ-1` Flash or RAM use over a threshold (default 90 % of the region) in a shipped profile · check: elf easy · <!-- tpm roles/firmware.md; vb135 PLAT-7; typec_dongle (99.6 %) -->
- <a id="siz-2"></a>⚠️ `SIZ-2` Dev profile not linked in CI: debug assertions and overflow checks add 18-30 KiB and overflow unnoticed · check: cfg easy · <!-- typec_dongle FW-53; vb135 PLAT-7 -->
- <a id="siz-3"></a>💡 `SIZ-3` Per-package `opt-level` together with `lto = "fat"`: the override does nothing · check: cfg easy · <!-- typec_dongle FW-57 -->
- <a id="siz-4"></a>💡 `SIZ-4` Release profile without `codegen-units = 1`, `lto`, `panic = "abort"`, and an `opt-level` chosen by measurement (z vs 3 is the largest lever: Donguru 501.7 → 321.8 KiB, 10-25 % slower USB) · check: cfg easy · <!-- typec_dongle FW-57; wire_weaver AGENTS.md -->
- <a id="siz-5"></a>💡 `SIZ-5` `core::fmt` pulled in: `Result::unwrap` / `expect` (through `&dyn Debug`), `Debug2Format`, a panic handler that formats `PanicInfo` (4-5 KiB) · check: src easy + elf easy · <!-- P2534 fw-size-2026-10-19 -->
- <a id="siz-6"></a>💡 `SIZ-6` Large `.data` initializer (a static with a non-zero const value): zero-init and set it up in place · check: elf easy · <!-- typec_dongle FW-42 (20 KiB encoder) -->
- <a id="siz-7"></a>💡 `SIZ-7` A shared crate turns on wide types or features for every firmware (`ww_si` u64: 7.7 KB) · check: cfg mid · <!-- wire_weaver ww_client_server -->
- <a id="siz-8"></a>💡 `SIZ-8` A cargo feature meant to cut code saves nothing (bodies gated, dispatch and ser/des not): measure what each feature saves · check: elf mid · <!-- P2534 fw-size debug-api -->
- <a id="siz-9"></a>💡 `SIZ-9` Generic code compiled once per const parameter (`ObserveTable<N>`), unique constants that stop identical arms from merging · check: elf hard · <!-- P2534 fw-size -->
- <a id="siz-10"></a>💡 `SIZ-10` Size comments in Cargo.toml or code without a date and profile: they go stale and mislead · check: review · <!-- typec_dongle Cargo.toml -->

## Robustness

### 💥 FLT - Faults, panics and resets
- <a id="flt-1"></a>🛑 `FLT-1` HardFault handler that spins (`loop {}`) in a shipped build: record (crash capture) and reset · check: src easy · <!-- embedded_bedrock ROB-2/ROB-11; usb_io_fw PLAT-7; notion -->
- <a id="flt-2"></a>🛑 `FLT-2` defmt, `cnt!`, `unwrap` or anything that can panic or nest-log inside `HardFault` / `DefaultHandler`: record the IRQ number raw instead · check: src easy · <!-- typec_dongle FW-48 -->
- <a id="flt-3"></a>⚠️ `FLT-3` `bedrock_crash::exc_return!()` not the first statement of HardFault (LR changes) · check: src easy · <!-- template main.rs -->
- <a id="flt-4"></a>⚠️ `FLT-4` No UsageFault, MemManage and BusFault handlers (or not enabled in SHCSR) where the core has them · check: src easy · chip: M3+ · <!-- embedded_bedrock ROB-4 -->
- <a id="flt-5"></a>⚠️ `FLT-5` Reset cause not read and reported at boot (power-on, brown-out, watchdog, software, pin, lockup) · check: src easy · <!-- notion; embedded_bedrock ROB-11 -->
- <a id="flt-6"></a>⚠️ `FLT-6` Crash record or RAM handoff read after a power-on or brown-out (RAM content undefined, ECC RAM faults) or without magic + CRC · check: review · <!-- bedrock_crash -->
- <a id="flt-7"></a>🛑 `FLT-7` No watchdog, or a watchdog fed from an interrupt or from a loop not bounded by a timeout; window not calibrated to the real period · check: src mid · <!-- notion -->
- <a id="flt-8"></a>⚠️ `FLT-8` No boot-loop rescue: count boots without a healthy mark in a backup register, fall back to the ROM bootloader after a few · check: src easy · <!-- typec_dongle FW-45; embedded_bedrock DEV-16 -->
- <a id="flt-9"></a>⚠️ `FLT-9` Healthy mark never set on the transport in use (`usb_configured()` not called with USB transport): every boot waits the full timeout · check: src mid · <!-- embedded_bedrock TPL-64 -->
- <a id="flt-10"></a>🛑 `FLT-10` Forced HardFault and panic not tried on hardware: the next boot reports them (`last reset: Fault / Panic`) with the ELF symbolized · check: bench · <!-- embedded_bedrock ROB-11 -->
- <a id="flt-11"></a>💡 `FLT-11` `exc_return` relied on for crash capture on Cortex-M0+ (reads 0 there) · check: review · chip: M0+ · <!-- typec_dongle FW-13 -->
- <a id="flt-12"></a>⚠️ `FLT-12` Runaway conditions not handled (AN1015): control registers of critical peripherals never re-written or checked periodically, illegal-opcode bytes in unused flash · check: review · <!-- notion -->
- <a id="flt-13"></a>⚠️ `FLT-13` Critical data used without a validity check (CRC, redundant copy, range) · check: review · <!-- notion -->

### 🚀 BOOT - Boot, ROM bootloader and updates
- <a id="boot-1"></a>🛑 `BOOT-1` Firmware entered by a jump (ROM DfuSe leave, bootloader) not detected: VTOR, PLL, SysTick, enabled IRQs of the ROM are still set; detect (RCC_CFGR SWS, VTOR, NVIC) and reset once · check: src mid · <!-- vb135 BOOT-6; typec_dongle FW-12 -->
- <a id="boot-2"></a>🛑 `BOOT-2` H7 supply config written blind: an old ROM bootloader (before V9.3) locks PWR_CR3 until a power cycle; read it back, use what it holds, count a mismatch · check: src mid · chip: H7 · <!-- vb135 BOOT-8; embedded_bedrock TPL-69 -->
- <a id="boot-3"></a>🛑 `BOOT-3` Control signal on a pin the ROM bootloader drives (AN2606: USART, USB, CAN, I2C pins of the ROM's interfaces) · check: review (board map) · <!-- typec_dongle FW-50 (PA9 cut DUT USB during DFU) -->
- <a id="boot-4"></a>🛑 `BOOT-4` No probe-less recovery path: ROM bootloader (BOOT pin, `boot reboot --updater`) or a DFU in the firmware must stay reachable, also after a crash at start-up · check: review · <!-- vb135 USB-2; template conventions -->
- <a id="boot-5"></a>🛑 `BOOT-5` Bootloader state written in an order a power cut can tear (embassy-boot STATE erase before zeroing the magic loses both images) · check: test (power-cut fuzz) · <!-- chunky_mcu_updater BOOT-5; embassy#7235 -->
- <a id="boot-6"></a>⚠️ `BOOT-6` A confirm or mark write that rewrites state when nothing changed · check: test · <!-- chunky_mcu_updater BOOT-1 -->
- <a id="boot-7"></a>🛑 `BOOT-7` Update not tried from the previous released version, over the field path (no probe) · check: bench · <!-- tpm roles/firmware.md release checklist -->
- <a id="boot-8"></a>⚠️ `BOOT-8` Wire format of the update path changed without a version bump: an older firmware must still understand `reboot --updater` · check: cfg mid (API snapshots) · <!-- typec_dongle FW-55 -->
- <a id="boot-9"></a>⚠️ `BOOT-9` Dual-bank bootloader passed `bank1_region` only while the DFU region sits in bank 2 · check: cfg mid · <!-- template stm32-notes -->
- <a id="boot-10"></a>⚠️ `BOOT-10` Backup-domain helper that turns the RTC clock or `PWR_CR1.DBP` off afterwards instead of restoring what it found · check: src mid · <!-- vb135 BOOT-5/6 (every boot unhealthy, ROM after 4) -->

### 🔋 PWR - Supply supervision and low power
- <a id="pwr-1"></a>⚠️ `PWR-1` No brown-out reset level or programmable voltage detector (PVD) for the supply the board really has · check: src easy · <!-- notion -->
- <a id="pwr-2"></a>🛑 `PWR-2` H7 SMPS / LDO supply config not the one of the schematic: the MCU may not start or may be damaged · check: cfg mid + review · chip: H7 · <!-- template stm32-notes -->
- <a id="pwr-3"></a>⚠️ `PWR-3` Backup domain not reset at boot when the RTC and backup registers are unused (corrupted after a brown-out without POR) · check: src mid · <!-- template stm32-notes (efton g133/g62) -->
- <a id="pwr-4"></a>⚠️ `PWR-4` RTC clock source changed in the clock config of a firmware that keeps state in the backup domain: embassy resets the domain · check: src mid · <!-- donguru_template main.rs -->
- <a id="pwr-5"></a>⚠️ `PWR-5` How long the MCU runs after a supply cut not measured where it matters (writes, erases): B129 G0 keeps running 60-70 ms · check: bench · <!-- tpm log Oct 2026 -->

### 🧷 FLS - Flash and ECC
- <a id="fls-1"></a>🛑 `FLS-1` Flash that a power cut can tear (settings, bootloader state) read raw on an ECC part: a double-ECC word is an NMI (G0) or BusFault (H7) on every boot; read through `ww_ecc_flash` with the NMI hook · check: src mid · chip: G0, H7, G4, L4... · <!-- typec_dongle API-19; template settings; embassy#7235 -->
- <a id="fls-2"></a>🛑 `FLS-2` More than one owner of the flash peripheral (updater and settings each take `FLASH`) · check: src mid · <!-- embedded_bedrock TPL-24 -->
- <a id="fls-3"></a>🛑 `FLS-3` RP flash erase or write from core 1, or with core 1 started outside `spawn_core1`, or while DMA reads flash (XIP stalls) · check: src mid · chip: RP · <!-- embedded_bedrock TPL-25 -->
- <a id="fls-4"></a>⚠️ `FLS-4` No periodic CRC of the image in flash, no ECC on flash and RAM where the part has it · check: src easy · <!-- notion -->
- <a id="fls-5"></a>💡 `FLS-5` Slow erase: check the programming parallelism (higher peak current) · check: review · <!-- notion -->
- <a id="fls-6"></a>⚠️ `FLS-6` Power-fail behaviour of the flash store not tested with cuts during erase and write on the family in use (L0/L1 erase to 0x00, U5 refuses zeros over zeros, H7 flash words programmed once) · check: test · <!-- chunky_mcu_updater fuzz; tpm log Oct 2026 -->
- <a id="fls-7"></a>⚠️ `FLS-7` Unused program memory not filled with a trap (or protected) · check: elf easy · <!-- notion -->

## Hardware

### 📌 PIN - Pins and IO
- <a id="pin-1"></a>🛑 `PIN-1` Pin driver dropped (`let _ = Output::new(..)`, a temporary, end of `main`'s scope): embassy disconnects the pin on drop · check: src easy · <!-- donguru_template main.rs; notion "destructors" -->
- <a id="pin-2"></a>🛑 `PIN-2` Board not put in its safe state first thing after the HAL init (output enables, shift-register chains, power switches), before USB or anything slow · check: src mid + review · <!-- usb_io_fw PLAT-9 (595 chain could short VBUS into 3V3) -->
- <a id="pin-3"></a>🛑 `PIN-3` A level translator or bus switch enabled before the MCU side is ready (pins released, at the right level, peripheral off); on last, off first · check: test + review · <!-- usb_io_fw IF-2 -->
- <a id="pin-4"></a>🛑 `PIN-4` Peripheral pins moved to GPIO and back under an enabled peripheral (RP2350: the peripheral reads the OR of all pins with its FUNCSEL; the I2C block saw a start without a stop): keep FUNCSEL, use OEOVER / OUTOVER · check: src mid · chip: RP · <!-- usb_io_fw IF-2, i2c_periph BE-1 -->
- <a id="pin-5"></a>⚠️ `PIN-5` Input relies on an external pull the BOM may not fit (DNP), with the pad pull off · check: review · <!-- usb_io_fw IF-14 -->
- <a id="pin-6"></a>🛑 `PIN-6` Two owners of a pin (a diagnostic or test API and a driver task): one owner per pin, the other refused · check: src hard · <!-- usb_io_fw IF-16 -->
- <a id="pin-7"></a>🛑 `PIN-7` Direction pin of a level translator turned while its IO pin drives: stop driving first · check: test · <!-- usb_io_fw IO-1 -->
- <a id="pin-8"></a>⚠️ `PIN-8` Unused pins left floating inputs: analog (STM32) or a defined pull · check: src mid · <!-- notion -->
- <a id="pin-9"></a>🛑 `PIN-9` Pin map chosen by a cargo feature for a board variant: a build for one variant flashed onto another drives the wrong lines; read the board record at run time, default to inputs · check: src mid · <!-- P2424 sd_card.md -->
- <a id="pin-10"></a>⚠️ `PIN-10` Lines driven onto a device that may be unpowered (back-feed), and not released to Hi-Z on every session end (disconnect, USB reset, suspend, cancel) · check: review · <!-- typec_dongle FW-17, API-30 -->
- <a id="pin-11"></a>⚠️ `PIN-11` Hardware feature used without checking the board record says the rework or variant is there · check: review · <!-- P2424 api_design (rework guard) -->
- <a id="pin-12"></a>⚠️ `PIN-12` Pin direction set only by an optional protocol command instead of on every path that uses it · check: review · <!-- typec_dongle 0.3.0 (SWCLK stayed input) -->
- <a id="pin-13"></a>⚠️ `PIN-13` A pin that must hold across an MCU reset driven by a GPIO (goes to reset state): use a backup-domain output (RTC_OUT) or the right pull · check: review · <!-- typec_dongle FW-46, FW-49 -->
- <a id="pin-14"></a>⚠️ `PIN-14` Input sampled once: filter by sampling several times where noise can fake an edge · check: review · <!-- notion -->
- <a id="pin-15"></a>⚠️ `PIN-15` IO speed, drive and load against the frequency used (edge rates, EMC) · check: review · <!-- notion -->
- <a id="pin-16"></a>⚠️ `PIN-16` Pin read that a dead-battery pull-down still holds (G0 UCPD DBCC pins until the SYSCFG strobe; never clear the strobe again) · check: src mid · chip: G0 · <!-- typec_dongle FW-22 -->

### 🕰️ CLK - Clocks and timers
- <a id="clk-1"></a>⚠️ `CLK-1` Core clock not measured once (DWT cycles against the time driver): prescaler math goes wrong silently (H7 at 270 instead of 540 MHz) · check: bench · <!-- vb135 PLAT-8 -->
- <a id="clk-2"></a>🛑 `CLK-2` embassy time driver `time-driver-any`, or the named timer also used for PWM or capture · check: cfg easy + src mid · chip: STM32 · <!-- template TPL-39; typec_dongle; b135_template -->
- <a id="clk-3"></a>⚠️ `CLK-3` No clock security: CSS on HSE (and LSE) with a handler · check: src easy · <!-- notion; embedded_bedrock ROB-5 -->
- <a id="clk-4"></a>⚠️ `CLK-4` LSE start-up (seconds on H7) blocks boot or a feature · check: review · <!-- notion -->
- <a id="clk-5"></a>🛑 `CLK-5` USB enabled on a part without HSI48 / CRS and no exact 48 MHz source configured · check: src mid · <!-- embedded_bedrock TPL-13 -->
- <a id="clk-6"></a>⚠️ `CLK-6` A kernel clock that can't divide the rates needed exactly (FDCAN 135 MHz for 2/4/8 Mbit/s, USART OVER8 drops a bit of USARTDIV): read back the achieved rate and count a mismatch · check: test · <!-- vb135 CAN-1; typec_dongle API-14 -->
- <a id="clk-7"></a>⚠️ `CLK-7` Clock run at the part's maximum just because it can (lifetime at small nodes, ECC RAM limits: H7 TCM ECC at 520 MHz) · check: review · <!-- notion; vb135 PLAT-8 -->
- <a id="clk-8"></a>⚠️ `CLK-8` Fractional PLL math not checked: embassy refuses a core above the limit at boot, a FRACN at 0 is integer mode, a truncated prescaler runs the time driver 0.39 % fast · check: test · chip: H7 · <!-- vb135 PLAT-9 -->

### 🐞 ERR - Silicon errata
- <a id="err-1"></a>🛑 `ERR-1` Errata sheet for the silicon revision on the board not read for the peripherals, clocks, flash, DMA, low power and boot path in use; STM32 also efton.sk gotchas · check: review · <!-- tpm firmware pack; embedded_bedrock AGENTS.md -->
- <a id="err-2"></a>🛑 `ERR-2` Register poke for an erratum without the erratum quoted (number, title, revisions, workaround) next to it · check: src easy · <!-- tpm firmware pack -->
- <a id="err-3"></a>🛑 `ERR-3` G0B0/G0B1/G0C1 flash prefetch on while code crosses into bank 2 (ES0548 2.2.10, ES0547 2.2.8: HardFault on cross-bank branches) · check: src mid + elf easy · chip: G0 dual bank · <!-- typec_dongle FW-38; template TPL-32 -->
- <a id="err-4"></a>🛑 `ERR-4` FDCAN edge filtering (`CCCR.EFBI`) on, or dedicated Tx buffers beside the Tx FIFO (ES0548 2.15.1/2.15.2, ES0491 2.22.1/2.22.2) · check: src mid · chip: G0, H7 · <!-- template errata.md; vb135 PLAT-6 -->
- <a id="err-5"></a>⚠️ `ERR-5` FDCAN DAR (one-shot) without retransmit after a lost arbitration on the first two ID bits (ES0491 2.22.3) · check: src mid · chip: H7 · <!-- vb135 PLAT-6 -->
- <a id="err-6"></a>⚠️ `ERR-6` RP2350 A2 pad with pull-down and input buffer left on (RP2350-E9: the pin stays high): enable the input only for the read · check: src mid · chip: RP2350 A2 · <!-- report_template; embedded_bedrock TPL-48 -->
- <a id="err-7"></a>⚠️ `ERR-7` RP2350 ROM reboot with delay 0 or FRCE_OFF bits beyond PROC1 (RP2350-E30, E19) · check: src easy · chip: RP2350 · <!-- template boot.rs -->
- <a id="err-8"></a>⚠️ `ERR-8` G0 ADC calibration without the wait of 2 ADC kernel clocks after disable (freezes) · check: src mid · chip: G0 · <!-- typec_dongle 0.20.7 (G0 gotcha 221) -->
- <a id="err-9"></a>⚠️ `ERR-9` I2C timing not checked against the kernel / APB clock ratio errata (ES0548 2.11.1, 2.11.4) · check: review · chip: G0 · <!-- donguru_template main.rs -->
- <a id="err-10"></a>⚠️ `ERR-10` Cache on without reading the cache errata of the part · check: review · <!-- notion -->
- <a id="err-11"></a>💡 `ERR-11` Silicon revision read from a register that can't tell all revisions apart (RP2350 `CHIP_ID.REVISION`: A3 and A4 both 3; use the bootrom version) · check: src easy · chip: RP2350 · <!-- embedded_bedrock TPL-55 -->

### 🔌 EXT - External chips and buses
- <a id="ext-1"></a>⚠️ `EXT-1` An I2C NAK taken as "device lost": chips NAK while busy; use a positive signature (ID reads all-zero twice) and a hold-off after attach · check: review · <!-- typec_dongle FW-41, FW-18 -->
- <a id="ext-2"></a>⚠️ `EXT-2` External chip on a rail that can sag without an MCU reset, and no watchdog that notices it lost its config (registers back at reset values) and brings it up again · check: review · <!-- typec_dongle FW-18; pd_bench FUSB-14 -->
- <a id="ext-3"></a>⚠️ `EXT-3` A configurable chip identified by its ID registers before it is configured (no factory default): use a strap · check: review · <!-- typec_dongle FW-22 -->
- <a id="ext-4"></a>⚠️ `EXT-4` Read-modify-write of an external chip's registers over the bus, clear-on-read registers read more than once: write from shadow copies · check: src mid · <!-- pd_bench AGENTS.md -->
- <a id="ext-5"></a>⚠️ `EXT-5` A sense line assumed wired (PHY VBUS pin, card detect polarity): read the board and the connector datasheet · check: review · <!-- P2424 ZY12PDN; vb135 SD-3 -->
- <a id="ext-6"></a>⚠️ `EXT-6` A settle or start-up time skipped on one path (VBUS after a hard reset, reset to first command, power-on to reset) · check: review · <!-- typec_dongle FW-60; notion -->
- <a id="ext-7"></a>⚠️ `EXT-7` Register field encoded without a range check (`x - 1` underflows, masked to 0) · check: src mid · <!-- mcan T1 -->
- <a id="ext-8"></a>⚠️ `EXT-8` Config value set but never written to the hardware by the apply path, or not stored and re-applied stale after a mode change · check: test · <!-- mcan F1, T2, S1 -->
- <a id="ext-9"></a>⚠️ `EXT-9` Shared peripheral memory (H7 FDCAN message RAM) zeroed whole by one instance's init · check: review · <!-- mcan R4 -->
- <a id="ext-10"></a>⚠️ `EXT-10` Register map or masks of another IP variant (G0 FDCAN with the H7 map, reserved bits in masks) · check: review · <!-- mcan P2 -->
- <a id="ext-11"></a>💡 `EXT-11` A HAL driver taken without checking it against what the product needs (timing accuracy, NAK index, recovery, panics in `new`) · check: review · <!-- P2424 i2c-master-driver (embassy-rp I2C) -->
- <a id="ext-12"></a>⚠️ `EXT-12` RCC enable not read back before the first access to the peripheral · check: src mid · chip: STM32 · <!-- donguru_template main.rs -->

### 🔗 USB - USB device side
- <a id="usb-1"></a>🛑 `USB-1` High-speed bulk endpoint max packet not 512, bInterval out of range for the speed · check: src mid · <!-- vb135 c0d19b8 (Windows) -->
- <a id="usb-2"></a>⚠️ `USB-2` A transfer that is an exact multiple of the max packet without a ZLP, or framing that waits for a short packet the host may not send · check: review · <!-- typec_dongle FW-19 -->
- <a id="usb-3"></a>💡 `USB-3` Single-buffered bulk endpoints on a hot path behind a hub TT (G0 usb_v4: 500 vs 947 kB/s) · check: src easy · chip: STM32 usb_v4 · <!-- typec_dongle FW-24 -->
- <a id="usb-4"></a>⚠️ `USB-4` Sending stuck when the host driver closes or the host dies without a disconnect · check: bench · <!-- vb135 bb820d9; typec_dongle FW-26 -->

## Interfaces and diagnostics

### 🛡️ HOST - Host input and the device API
- <a id="host-1"></a>🛑 `HOST-1` Panic reachable from host input: `unwrap`, `expect`, slice indexing, `unreachable!`, arithmetic overflow on values from a request (pin numbers, lengths, rates) · check: src hard · <!-- usb_io_fw AGENTS.md; typec_dongle FW-19 (dap_rs indexed past a truncated request) -->
- <a id="host-2"></a>🛑 `HOST-2` Host can drive a pin into a level or mode that damages the board: every electrical rule enforced by the firmware, refused with an error · check: review · <!-- usb_io_fw AGENTS.md -->
- <a id="host-3"></a>⚠️ `HOST-3` Unknown enum value from a newer host mapped to a default (`_ => ..`) instead of refused · check: src mid · <!-- usb_io_fw API-5 -->
- <a id="host-4"></a>⚠️ `HOST-4` A host that stops reading can stall a release or cleanup path: every wait on the host has a timeout · check: review · <!-- typec_dongle FW-19 -->
- <a id="host-5"></a>⚠️ `HOST-5` Truncated or oversized request not counted and dropped · check: src mid · <!-- typec_dongle FW-19 -->
- <a id="host-6"></a>⚠️ `HOST-6` A host-settable range wider than the firmware can serve (bounded by DMA geometry, buffers): limits derived from the same constants on both sides · check: review · <!-- typec_dongle API-12 -->
- <a id="host-7"></a>⚠️ `HOST-7` One unimplemented field voids a batched update; a deferred answer path (credit, grant) only sent from inside the handler · check: test · <!-- typec_dongle SRV-24; usb_io_fw IF-13 -->
- <a id="host-8"></a>⚠️ `HOST-8` Loss reported out of order with the data it belongs to · check: test · <!-- typec_dongle 0.20.14 -->

### 📜 LOG - Logging
- <a id="log-1"></a>🛑 `LOG-1` RTT that can block: probe-rs sets the channel to blocking on attach; force NoBlockSkip, whole frames or skipped and counted · check: src mid · <!-- typec_dongle FW-27; wire_weaver STD-26; embedded_bedrock ROB-8 -->
- <a id="log-2"></a>⚠️ `LOG-2` `DEFMT_RTT_BUFFER_SIZE` and `DEFMT_LOG` not set explicitly in `.cargo/config.toml` (init bursts lost: 773 of 1000 lines) · check: cfg easy · <!-- template conventions; tpm log Oct 2026 -->
- <a id="log-3"></a>💡 `LOG-3` Hot-path logging at info or above · check: review

### 🔢 CNT - Counters
- <a id="cnt-1"></a>⚠️ `CNT-1` Error branch (an `Err` handled, a timeout, a retry, a dropped frame, an overrun) without a `cnt!` counter: unexpected events must be counted · check: src hard · <!-- template conventions; typec_dongle FW-62 (pd_rx_overrun added after) -->
- <a id="cnt-2"></a>⚠️ `CNT-2` `&mut` to a counter buffer that an interrupt can update, unaligned u64 access · check: src mid · <!-- cnt CHANGELOG -->
- <a id="cnt-3"></a>⚠️ `CNT-3` Proc-macro crate and its runtime crate not pinned to the exact same version (`=x.y.z`) · check: cfg easy · <!-- cnt -->

## Build and process

### ⚙️ CFG - Build configuration
- <a id="cfg-1"></a>🛑 `CFG-1` probe-rs runner or a recipe without `--probe VID:PID`: with two probes on the PC another board gets flashed · check: cfg easy · <!-- nucleo_g0b1re_template; template conventions -->
- <a id="cfg-2"></a>⚠️ `CFG-2` `probe-rs download` then `reset` on F0/G0/L0: probe-rs writes RCC_APBENR1 back on a running core (USB clock off); use a runner that halts first · check: cfg easy · chip: F0, G0, L0 · <!-- typec_dongle FW-41 -->
- <a id="cfg-3"></a>⚠️ `CFG-3` Feature sets not all built and linted (default, `--all-features`, `--no-default-features`): gated code rots · check: cfg easy · <!-- typec_dongle FW-23 -->
- <a id="cfg-4"></a>🛑 `CFG-4` A crate in the tree twice (git and path copies of WireWeaver, embassy): patch it to one source · check: cfg easy · <!-- typec_dongle FW-51 -->
- <a id="cfg-5"></a>🛑 `CFG-5` Chip feature (package, flash size) of the HAL not the part on the board · check: cfg mid · <!-- usb_io_fw 19203e6 (rp235xb) -->
- <a id="cfg-6"></a>⚠️ `CFG-6` thumbv6m without `portable-atomic` + `critical-section` where a crate needs CAS (`static_cell`) · check: cfg easy · chip: M0+ · <!-- mcan P2 -->
- <a id="cfg-7"></a>⚠️ `CFG-7` Dirty build flashed without a mark: build info carries the git SHA and a dirty flag, read back from the device · check: cfg easy · <!-- notion; embedded_bedrock BLD -->
- <a id="cfg-8"></a>⚠️ `CFG-8` Git consumers not built: a dependency from git (not path) breaks where path builds pass · check: cfg mid · <!-- embedded_bedrock TPL-43, BLD-8 -->

### ⚡ PERF - Speed
- <a id="perf-1"></a>⚠️ `PERF-1` Speed or size claims without numbers at the opt-level that ships, in their own units with the clock (instructions in QEMU, cycles with DWT) · check: review · <!-- tpm firmware pack; wire_weaver AGENTS.md -->
- <a id="perf-2"></a>💡 `PERF-2` 64-bit or i128 maths, float formatting, or `%` / `/` by a runtime value per element on a hot path of a core without them (`__aeabi_lmul`, `__aeabi_uldivmod`, `__multi3`) · check: elf mid · chip: M0+ · <!-- embedded_bedrock DEV-12/13/14; typec_dongle FW-42; P2534 fw-size -->
- <a id="perf-3"></a>⚠️ `PERF-3` RAM function (`#[link_section = ".data.*"]`) without `#[inline(never)]`: LTO inlines it back into flash · check: src easy + elf easy · <!-- typec_dongle FW-42 -->
- <a id="perf-4"></a>💡 `PERF-4` Hot path through a bitwise CRC, a bit-by-bit serializer, or GPIO wrappers per edge (embassy `Flex` ~50 cycles): measure before blaming the bus · check: review · <!-- usb_io_fw IF-12; typec_dongle FW-42, FW-20 -->
- <a id="perf-5"></a>💡 `PERF-5` Wake cost of an event loop with nothing to do (`select4` with timers and waker registrations per turn on M0+) · check: review · <!-- typec_dongle FW-40 -->

### 🧪 TEST - Tests and bring-up process
- <a id="test-1"></a>⚠️ `TEST-1` Host test that races a free-running tick or sleeps "long enough" · check: review · <!-- usb_io_fw API-51, HOST-6 -->
- <a id="test-2"></a>⚠️ `TEST-2` A simulator that answers inline hides bugs of the deferred path: test both · check: review · <!-- usb_io_fw IF-13 -->
- <a id="test-3"></a>⚠️ `TEST-3` Sequencing rules of the board (enable order, safe off) without host tests of the transition model · check: test · <!-- usb_io_fw IF-2 (chain::tests) -->
- <a id="test-4"></a>⚠️ `TEST-4` A board left running its last test after probe-rs exits: reset before each run · check: review · <!-- mcan Q5a -->
- <a id="test-5"></a>⚠️ `TEST-5` One probe, one owner: another session's probe-rs or gdb on the same board makes faults that aren't the firmware's · check: review · <!-- typec_dongle FW-44, FW-48 -->
- <a id="test-6"></a>💡 `TEST-6` A fault that depends on timing or layout bisected before the errata were checked · check: review · <!-- tpm firmware pack; typec_dongle FW-38 -->
- <a id="test-7"></a>⚠️ `TEST-7` A peripheral or chip blamed before our own side was measured (M1 blamed the sink's FUSB302B for our RXDR overrun) · check: review · <!-- typec_dongle FW-62 -->

## See also

- AN1015 "Software techniques for improving microcontrollers EMC performance"
- AN3307 "Guidelines for obtaining IEC 60335 Class B certification for any STM32 application", AN4435 (UL/CSA/IEC
  60335 Class B)
- http://www.efton.sk/STM32/gotcha/ (STM32 pitfalls); the errata the template handles:
  `firmware_template_skill/references/errata.md`
- better_erc's catalogue (`~/git/better_erc/docs/checks.md`), the same structure for schematics
