# SIZ checks, explained

Detail page for the `SIZ` category of the [check catalogue](../checks.md#-siz---flash-and-ram-budget). Each
section explains one check: what it catches, why the firmware fails without it, a concrete example, how it is
checked, and the fix. The one-line entries in the catalogue stay the source of truth for ID, severity and wording;
this page is the commentary.

## `SIZ-1` Flash or RAM use over threshold in a shipped profile <a id="siz-1"></a>

⚠️ warning · check: elf easy · [catalogue entry](../checks.md#siz-1)

**What it catches.** A shipped build whose flash or RAM use is over a threshold (default 90 %) of its region, read
from the linked ELF.

**Why it matters.** Past 90 % there's little headroom left for the next feature, the next dependency bump, or a
safety margin the project may need for things like `STK-6`'s stack budget; finding this out at 99.6 % full, mid
release, leaves no room to fix it quickly.

**Example.** vb135 PLAT-7, and typec_dongle at 99.6 % flash — both found after the fact rather than caught by a
budget check at build time.

**How to check.** `elf easy`: sum section sizes per memory region from the linked ELF and compare against the
region's size.

**Fix.** Nothing automatic — this check exists to surface the number early; reducing use is `SIZ-4` through
`SIZ-9`'s job, or the budget needs raising (a bigger part, or dropping `--settings`/`--bootloader` headroom
elsewhere).

## `SIZ-2` Dev profile not linked in CI <a id="siz-2"></a>

⚠️ warning · check: cfg easy · [catalogue entry](../checks.md#siz-2)

**What it catches.** CI building and testing only the release profile, never linking the dev profile (debug
assertions, overflow checks on) for the actual target.

**Why it matters.** Debug assertions and overflow checks add real size (18-30 KiB) and, more importantly, catch
real bugs (an overflow that release silently wraps); if CI never links dev for the target, an overflow introduced
on a branch goes unnoticed until someone happens to flash a dev build locally, if ever.

**Example.** typec_dongle FW-53 and vb135 PLAT-7 both found overflow-class bugs only after someone linked dev by
hand, well after the code had shipped in release.

**How to check.** `cfg easy`: CI configuration (the justfile recipes, GitHub Actions config) must include a dev
profile build for the firmware's target, not only `--release`.

**Fix.** Add a dev-profile build/link step to CI for every firmware target, even if it isn't flashed.

## `SIZ-3` Per-package `opt-level` with `lto = "fat"` <a id="siz-3"></a>

💡 suggestion · check: cfg easy · [catalogue entry](../checks.md#siz-3)

**What it catches.** A `[profile.release.package.<name>]` override of `opt-level` in a workspace that also sets
`lto = "fat"` on the top-level profile.

**Why it matters.** Fat LTO re-optimises the whole program as one unit after per-crate compilation, discarding the
per-package `opt-level` the override asked for; the override costs nothing but also does nothing, which is the
quiet failure mode here — not a bug, just wasted Cargo.toml and a false sense of control over size.

**Example.** typec_dongle FW-57: a per-package override that looked deliberate turned out to have no effect once
fat LTO was already on.

**How to check.** `cfg easy`: a per-package `opt-level` override present alongside `lto = "fat"` in the same
profile.

**Fix.** Drop the override (fat LTO already re-optimises everything), or switch to `lto = "thin"`/no LTO if the
per-package control is actually needed.

## `SIZ-4` Release profile missing the standard size levers <a id="siz-4"></a>

💡 suggestion · check: cfg easy · [catalogue entry](../checks.md#siz-4)

**What it catches.** A release profile missing `codegen-units = 1`, `lto` (thin or fat), `panic = "abort"`, or an
`opt-level` chosen without measuring it against the alternative (`z` vs `3` is the largest lever of the group).

**Why it matters.** These are the standard, well-understood size/speed trade-offs for embedded Rust; skipping them
isn't wrong so much as leaving an easy, already-known-safe win on the table, and picking `opt-level` by habit
instead of measurement can go either way depending on the actual code (size vs speed is not monotonic).

**Example.** typec_dongle FW-57 and the Donguru measurement wire_weaver's AGENTS.md cites: `opt-level = "z"`
brought one firmware from 501.7 to 321.8 KiB, at a real cost of 10-25 % more USB latency — a trade worth making
deliberately, not by default.

**How to check.** `cfg easy`: the four settings' presence in `Cargo.toml`'s release profile; whether `opt-level`
was *measured* isn't itself automatable, hence the suggestion severity.

**Fix.** Set `codegen-units = 1`, `lto = "fat"` (or `"thin"` if build time matters more), `panic = "abort"`; measure
`opt-level = "z"` against `"3"` on the actual firmware before picking one.

## `SIZ-5` `core::fmt` pulled in by `unwrap`/`expect`/`Debug2Format` <a id="siz-5"></a>

💡 suggestion · check: src easy + elf easy · [catalogue entry](../checks.md#siz-5)

**What it catches.** `Result::unwrap`/`.expect(...)` (which formats through `&dyn Debug` on panic),
`defmt::Debug2Format`, or a panic handler that formats the full `PanicInfo`, each of which pulls in `core::fmt`'s
formatting machinery (4-5 KiB) even when nothing else in the firmware needs it.

**Why it matters.** `core::fmt` is sized once it's reachable at all, regardless of how rarely the formatting path
actually runs; on a size-constrained build this fixed cost for an error path nobody wants to see output from in
production is often not worth paying.

**Example.** P2534's `fw-size-2026-10-19` measurement found exactly this pulling in formatting code on a build
that otherwise avoided it everywhere else.

**How to check.** `src easy` (the syntactic pattern: `unwrap`/`expect`/`Debug2Format` call sites) plus `elf easy`
(confirm `core::fmt` symbols are actually present/absent in the linked output).

**Fix.** Replace `unwrap`/`expect` with explicit error handling or `unwrap_or`-style defaults on hot/size-critical
paths; keep a panic handler that records raw data (`FLT-2`) instead of formatting `PanicInfo`.

## `SIZ-6` Large `.data` initializer <a id="siz-6"></a>

💡 suggestion · check: elf easy · [catalogue entry](../checks.md#siz-6)

**What it catches.** A `static` with a sizeable non-zero const initializer, which the linker places in `.data` —
meaning its full initial contents are stored in flash *and* copied into RAM at startup, doubling its footprint
instead of being zero-initialised and built up in place.

**Why it matters.** `.data`'s initial image costs flash exactly as much as `.bss` would cost nothing; for anything
large (a lookup table, an encoder's working buffer) that's flash spent on a value the code is about to overwrite
or compute anyway.

**Example.** typec_dongle FW-42: a 20 KiB encoder buffer with a non-zero initializer, found during a size pass.

**How to check.** `elf easy`: a `.data` symbol over a size threshold whose initial bytes aren't all zero (which
would have gone in `.bss` instead).

**Fix.** Zero-initialise the static (`.bss`) and fill it at startup in code instead of baking a large constant into
the binary.

## `SIZ-7` A shared crate turning on wide types for every firmware <a id="siz-7"></a>

💡 suggestion · check: cfg mid · [catalogue entry](../checks.md#siz-7)

**What it catches.** A shared crate (WireWeaver's `ww_si`, for instance) defaulting to a wide integer type (`u64`)
across every firmware that depends on it, rather than letting each firmware pick the width it actually needs.

**Why it matters.** 64-bit arithmetic on a core without a hardware multiplier/divider for it (most M0+) is
expensive both in flash (software routines, `PERF-2`) and in cycles; a shared default that's wide "to be safe" taxes
every firmware that never needed more than 32 bits.

**Example.** wire_weaver's `ww_client_server`: `ww_si`'s `u64` default cost 7.7 KB on firmware that only ever used
small indices.

**How to check.** `cfg mid`: compare a shared dependency's selected feature/type width against what the firmware
actually stores through it.

**Fix.** Make the width a feature the firmware chooses, defaulting to the smallest type that fits the actual data,
not the widest "to be safe".

## `SIZ-8` A size-cutting feature that saves nothing <a id="siz-8"></a>

💡 suggestion · check: elf mid · [catalogue entry](../checks.md#siz-8)

**What it catches.** A cargo feature meant to shrink the image (gating a handler's body, say) that measurably
saves little or nothing, because the dispatch table and ser/des machinery around the gated code are *not* gated
along with it.

**Why it matters.** A feature that looks like it should save size but doesn't adds complexity (two build
configurations to test, `CFG-3`) for no actual benefit; worse, it can hide the real cost, which stays in the
dispatch/ser-des layer regardless of the flag.

**Example.** P2534's `fw-size` measurement of the `debug-api` feature found this directly: gating handler bodies
without gating the dispatch table saved almost nothing.

**How to check.** `elf mid`: build with and without the feature and diff section sizes; a feature whose on/off
delta is far smaller than its gated code's own size is the signal.

**Fix.** Gate the whole vertical slice (dispatch entry, ser/des, handler body) or don't bother with the feature at
all.

## `SIZ-9` Generic code instantiated once per const parameter <a id="siz-9"></a>

💡 suggestion · check: elf hard · [catalogue entry](../checks.md#siz-9)

**What it catches.** A generic type parameterised over a `const N` (`ObserveTable<N>`) monomorphised separately
for each distinct `N` used, where the per-instance code could otherwise be identical and merged by the linker's
identical-code-folding — except each instance embeds its own unique constant, which stops the arms from being
byte-identical.

**Why it matters.** Monomorphisation already duplicates code per type parameter; when each copy additionally
differs only by a baked-in constant, the linker can't fold them even though the *logic* is the same, and the
duplication scales with how many distinct `N` the firmware instantiates.

**Example.** P2534's `fw-size` investigation traced a chunk of duplicated code directly to this pattern on
`ObserveTable<N>`.

**How to check.** `elf hard`: needs to recognise near-duplicate functions in the linked ELF that differ only by an
embedded constant — genuinely hard without disassembly-level diffing.

**Fix.** Factor the constant out as a runtime parameter (a field, not a const generic) where the value doesn't
need to be known at compile time, so one instantiation serves every `N`.

## `SIZ-10` Stale size comments <a id="siz-10"></a>

💡 suggestion · check: review · [catalogue entry](../checks.md#siz-10)

**What it catches.** A comment in `Cargo.toml` or code citing a size number without the date and profile it was
measured at.

**Why it matters.** Size numbers drift with every toolchain update, dependency bump and code change; a comment
that just says "saves 4 KB" with no date or profile can't be told apart from one that's still accurate and one
that's years stale, and either way it misleads the next person who reads it as current.

**Example.** A size comment in typec_dongle's `Cargo.toml` that needed its claim re-verified before it could be
trusted again.

**How to check.** Not automated (`review`): a size claim in a comment without an adjacent date and profile.

**Fix.** Date every size comment and name the profile it was measured at (e.g. "4.1 KiB saved, release/z, 19 Oct
2026"), and re-measure before relying on an old one.

## See also

- [Review checklist](../review-checklist.md) — *Numbers* and *Build* condense SIZ-1, SIZ-2 and SIZ-4.
- [`EXE-12`](exe.md#exe-12) — a flash-size finding from the same `fw-size` measurements `SIZ-5`/`SIZ-8`/`SIZ-9`
  reference.
- [`PERF-2`](../checks.md#perf-2) — the cycle-cost side of wide arithmetic that `SIZ-7` looks at from the flash
  side (its own detail page is LINT-8).
