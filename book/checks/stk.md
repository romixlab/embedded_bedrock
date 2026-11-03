# STK checks, explained

Detail page for the `STK` category of the [check catalogue](../checks.md#-stk---stack). Each section explains one
check: what it catches, why the firmware fails without it, a concrete example, how it is checked, and the fix. The
one-line entries in the catalogue stay the source of truth for ID, severity and wording; this page is the
commentary.

## `STK-1` flip-link not the linker <a id="stk-1"></a>

🛑 error · check: cfg easy · [catalogue entry](../checks.md#stk-1)

**What it catches.** `.cargo/config.toml`'s `[target.<triple>]` missing `linker = "flip-link"`, so the firmware
links with the plain linker instead.

**Why it matters.** flip-link moves the stack below `.data`/`.bss` so a stack overflow runs into an unmapped guard
instead of silently corrupting the first bytes of RAM. Without it, an overflow corrupts whatever static data
happens to sit above the stack and the symptom is wherever that data is read next — rarely at the overflow site.

**Example.** Found independently in embedded_bedrock (ROB-12), typec_dongle (FW-1) and usb_io_fw (PLAT-1): three
projects hit the same missing-linker mistake before it became a generator default and a lint.

**How to check.** `cfg easy`: read `.cargo/config.toml` for the target triple in use and check `linker` is set to
`"flip-link"`.

**Fix.** Add `linker = "flip-link"` under `[target.<triple>]`; the generator sets this by default for every
firmware it produces.

## `STK-2` Unknown `cargo:` key printed by build.rs <a id="stk-2"></a>

🛑 error · check: cfg easy · [catalogue entry](../checks.md#stk-2)

**What it catches.** A build script that prints `cargo:rustc-linker=...` (or any other `cargo:` key Cargo doesn't
recognise) expecting it to take effect.

**Why it matters.** Cargo silently ignores a build-script directive it doesn't recognise — there is no warning, no
error, nothing in the build log. A build.rs that means to set the linker this way builds and links fine, just with
whatever linker `.cargo/config.toml` actually specifies, which may not be flip-link (`STK-1`).

**Example.** embedded_bedrock ROB-12: a build.rs attempt to set the linker via a `cargo:` print did nothing, and
flip-link's absence was only found later by its real symptom.

**How to check.** `cfg easy`: scan `build.rs` for `cargo:` prints against the set of keys Cargo actually
recognises, and flag anything outside it.

**Fix.** Set the linker in `.cargo/config.toml` (`STK-1`), not from build.rs; use build.rs only for directives
Cargo documents.

## `STK-3` Stack not below `.data`/`.bss`, or zero-size stack <a id="stk-3"></a>

🛑 error · check: elf easy · [catalogue entry](../checks.md#stk-3)

**What it catches.** In the linked ELF, `_stack_start` not equal to the start of `.data` (flip-link's whole point
is to place the stack immediately below it), or a stack region that links to zero bytes.

**Why it matters.** This is `STK-1`'s outcome check: flip-link being *configured* doesn't guarantee it actually
placed the stack correctly for this particular memory layout — a zero-size stack region or a stray symbol between
stack and `.data` defeats the guard just as completely as not using flip-link at all.

**Example.** embedded_bedrock ROB-12, and a flip-link 0.1.12 bug where an empty `.sram8` section at the very end of
RAM was counted as used space, leaving no room for the stack at all.

**How to check.** `elf easy`: read `_stack_start` and the start address of `.data` directly from the linked ELF's
symbol table and compare; check the stack region's computed size is non-zero.

**Fix.** If the addresses don't line up, the memory.x region layout has an extra symbol or a non-empty tail between
stack and `.data` (`MEM-2`); fix the layout, not the linker flag.

## `STK-4` `paint-stack` turned on before BusFault is understood <a id="stk-4"></a>

🛑 error · check: cfg easy · [catalogue entry](../checks.md#stk-4)

**What it catches.** cortex-m-rt's `paint-stack` feature enabled in a firmware whose BusFault handling hasn't been
worked through yet.

**Why it matters.** `paint-stack` writes a known pattern across the stack region at boot so stack *high-water
mark* can be measured later; on some memory layouts or RAM types, painting memory that isn't backed the way the
tool assumes can itself trigger a BusFault before main() even runs, which is a confusing failure to debug if
BusFault handling isn't already solid.

**Example.** usb_io_fw commit a01319a and embedded_bedrock ROB-6 both hit this turning the feature on too early in
a new firmware's life.

**How to check.** `cfg easy`: `paint-stack` enabled in `Cargo.toml`/features, cross-checked against whether a
BusFault handler exists and is understood (`FLT-4`).

**Fix.** Get BusFault handling and reporting working first (`FLT-4`, `FLT-10`), then turn `paint-stack` on for
measurement.

## `STK-5` Big state built by value then moved into a static <a id="stk-5"></a>

⚠️ warning · check: src mid + elf mid · [catalogue entry](../checks.md#stk-5)

**What it catches.** A large struct constructed on the stack (`Big::new()`) and then moved whole into a
`StaticCell::init(...)`, instead of being built directly inside the static's storage.

**Why it matters.** Rust doesn't guarantee the move is elided; the calling function's stack frame has to hold the
whole value, sized as if the static didn't exist, right at the moment it's about to be thrown away. On a task
that's already close to its stack budget this can be the difference between fitting and overflowing, and it only
shows up as a size the task takes just before handing the data off.

**Example.** wire_weaver's `ObserveBatches`: a 31.6 KB stack frame on M4F, entirely from building the value before
moving it into its static.

**How to check.** `src mid` (spot the by-value construction pattern) plus `elf mid` (confirm the frame size in the
linked ELF) — both halves needed to tell a real problem from a value the compiler already elided.

**Fix.** Build the state in place: a `const fn new()` plus `MaybeUninit`-based initialisation directly inside the
static's storage, so no full copy ever exists on the stack.

## `STK-6` Stack frame or worst-case depth over budget <a id="stk-6"></a>

⚠️ warning · check: elf hard · [catalogue entry](../checks.md#stk-6)

**What it catches.** A single function's stack frame, or a function's worst-case call-chain depth, exceeding a
budget — either a per-function limit or the task/handler's share of the stack region.

**Why it matters.** This is the general case `STK-5` and `EXE-11` are specific instances of: without a budget and a
way to check it, "does this fit" is only answered by whether it overflowed on hardware (`STK-7`), which is a much
more expensive way to find out.

**How to check.** `elf hard`: needs the linked ELF's call graph and per-function frame sizes, summed along every
path from a task's entry point, including interrupt executors stacking on top of thread mode (`EXE-11`).

**Fix.** Reduce the deepest call chain's frame sizes (fewer large locals, less by-value state per `STK-5`), or
raise the stack region's budget if the margin was simply too tight.

## `STK-7` Stack overflow never provoked on hardware after a RAM layout change <a id="stk-7"></a>

🛑 error · check: bench · [catalogue entry](../checks.md#stk-7)

**What it catches.** A RAM layout change (new region, resized stack, a different `main_ram` choice) shipped without
deliberately provoking a stack overflow on real hardware to confirm it still ends in a clean fault.

**Why it matters.** `STK-1`/`STK-3` check that flip-link is configured and placed correctly, but only running an
actual overflow on the actual board proves the guard page does what it's supposed to — catch it in a fault, not
let it quietly corrupt `.data` the way it would without flip-link at all.

**Example.** embedded_bedrock ROB-1: part of establishing the guard-page convention was confirming on hardware that
an intentional overflow actually faults instead of corrupting memory.

**How to check.** Not automated (`bench`): deliberately recurse or allocate past the stack budget on target and
confirm the result is a fault report, not corrupted data or a silent hang.

**Fix.** N/A — this *is* the check; if it doesn't fault cleanly, go back to `STK-1`/`STK-3`/`MEM-2`.

## See also

- [Review checklist](../review-checklist.md) — *Memory and flash* condenses STK-1, STK-2, STK-3 and STK-5.
- [Bring-up checklist](../bringup-checklist.md) step 4 covers `STK-7` directly.
- [`MEM-2`](mem.md#mem-2) — the memory.x mistake that can defeat `STK-3` even with flip-link configured.
- [`EXE-11`](exe.md#exe-11) — stack budgeting after a task moves onto an interrupt executor.
