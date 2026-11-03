# ⚙️ CFG — Build configuration

Detail for the `CFG` checks of the [check catalogue](../checks.md).

## <a id="cfg-1"></a>`CFG-1` — a flash recipe without `--probe VID:PID`

A `.cargo/config.toml` runner or a `just flash` / `just run` recipe that calls `probe-rs run` or
`probe-rs download` without pinning the probe by its VID:PID flashes whichever probe-rs happens to pick first.
With one probe and one board on the PC nothing shows; plug in a second board for another firmware (or let a
sibling session's probe sit attached) and the wrong board gets flashed, or reset mid-session.

**Example:** the generated template's `.cargo/config.toml.j2` already gets this right and is the reference: its
runner line is `probe-rs run --chip {chip} --probe {probe}{extra}`, with the comment "Only {probe_name}
({probe}), never another probe on the PC: one with someone else's board would be flashed." and a second note
that `just run` / `just cnt` add the probe's own serial for a PC with two of the same board. A hand-written
recipe that drops the `--probe` flag (copied from an older README, or from `cargo-flash` examples that assume a
single probe) loses that guarantee silently.

**Fix:** always carry `--probe VID:PID` (or the probe's serial, for two identical boards) through every runner
and recipe; never rely on "there's only one probe right now."

**Detection:** check: cfg easy — a lint over `.cargo/config.toml` and the justfile's flash/run targets greps for
`probe-rs (run|download)` without a following `--probe`.

## <a id="cfg-2"></a>`CFG-2` — `probe-rs download` + `reset` turns a running core's clocks off (F0/G0/L0)

On STM32 F0/G0/L0 (Cortex-M0, ARMv6-M), probe-rs's unlock sequence for these parts reads RCC_APBENR1 over SWD and
writes it back verbatim as part of enabling the debug domain. If the firmware is already running and has turned
on peripheral clocks (USB, SPI, ...) between that read and that write, the write silently clears them again:
the core keeps running, but USB and whatever else was enabled in that window goes dark.

**Example:** typec_dongle FW-41 hit this on a NUCLEO-style G0B1 board: after `probe-rs download` + `probe-rs
reset` over J-Link, a USB hub downstream reset itself every 0.6 s and the firmware never enumerated, while
`cnt run` (which halts the core first) came up clean every time. RCC_APBENR1 came back `0x18410504` instead of
the expected `0x18416504` — USB and SPI2's clock bits missing — with every USB register reading zero and no D+
pull-up. It hit about one boot in three after a plain `probe-rs reset`; a cold power-up without a debugger
attached never saw it. The upstream report is at
`~/git/tpm/prj/P2534-donguru/design/upstream-probe-rs-g0-dbgen.md` (probe-rs's `debug_device_unlock`, affects
ARMv6-M STM32 parts generally, not fixed upstream yet as of this writing).

**Fix:** use a runner that halts the core before the unlock sequence runs (`cnt run`, or any `probe-rs` flow that
resets-and-halts instead of reset-while-running) on F0/G0/L0 targets; don't chain a bare `probe-rs reset` after
`download` on these chips.

**Detection:** check: cfg easy, limited to chip families F0/G0/L0 — a lint flags a runner or recipe that pairs
`probe-rs download` with a `probe-rs reset` (rather than a halting runner) for these chips specifically.

## <a id="cfg-3"></a>`CFG-3` — only one feature set built and linted

A crate with cargo features (debug-only diagnostics, an optional driver, `--all-features` demos) that is only
ever built and clippy'd with its default feature set lets the other combinations rot: a feature-gated module
can stop compiling, or a `cfg`-gated branch can silently diverge from the default path, with nothing catching
it until someone actually reaches for that feature during a release crunch.

**Example:** typec_dongle's FW-23 fix made `just lint` run `cargo fmt --check` and
`cargo clippy --bins --examples -D warnings` three times over `fw/donguru` — default features,
`--all-features`, and `--no-default-features` — specifically so a feature-gated path that stops building fails
CI instead of being discovered later. The thirteen lints outstanding on 15 Oct 2026 were down to doc-list
indentation and one dead-code warning by 24 Oct, once every combination ran.

**Fix:** lint (and ideally build/test) every meaningful feature combination in CI, not just the default one;
`default`, `--all-features`, and `--no-default-features` is the cheap baseline that catches most rot.

**Detection:** check: cfg easy — a CI config or justfile lint recipe is checked for invoking `cargo clippy`
(or `cargo build`) more than once across these three feature combinations; a single invocation is the finding.

## <a id="cfg-4"></a>`CFG-4` — a crate pulled into the tree twice, once by git and once by path

When one dependency (WireWeaver, embassy) is reachable both as a `git` source and as a `[patch]`-redirected
`path` source at the same time — because a sibling crate's own `Cargo.toml` still points at git while the
workspace root patches it to the local checkout — Cargo ends up compiling and linking two copies. Types that
look identical (`ww_indication::Indication`, `shrink_wrap`'s traits) become two distinct types to the compiler,
and every call between the two copies is a type error.

**Example:** typec_dongle FW-51: the workspace stopped building on 24 Oct 2026 because `bedrock_crash` came in
through embedded_bedrock's own `git` dependency (which itself pulled a templated `bedrock_template/Cargo.toml`),
while `bedrock_led` and `bedrock_sync` brought WireWeaver a *second* time from git on top of the workspace's
`[patch]` to the sibling `../wire_weaver` checkout — thirteen `E0308` type mismatches in `ww/leds.rs` and
`ww/clock.rs`, all "expected `shrink_wrap::X`, found `shrink_wrap::X`" from the two copies. The fix made
`bedrock_crash` a path dependency on `../embedded_bedrock`, like the other bedrock crates, and patched
`fw/donguru/Cargo.toml`'s WireWeaver source to the same sibling checkout that `bedrock_led`'s own `Cargo.toml`
already asked for.

**Fix:** patch every crate in the tree to exactly one source (git xor the sibling path) in the workspace root's
`[patch]`, and check every crate that depends on it transitively agrees — a sibling crate's own `Cargo.toml`
dependency line can reintroduce the git copy even with a correct root patch.

**Detection:** check: cfg easy — `cargo tree -d` (duplicate dependencies) catches this directly; the lint flags
any dependency present at two different git/path sources in the resolved tree.

## <a id="cfg-5"></a>`CFG-5` — the HAL's chip feature doesn't match the part on the board

HAL crates like `embassy-rp` gate the exact package variant (flash size, pin count, die) behind a cargo feature
(`rp235xa` vs `rp235xb`), and the wrong one builds and links cleanly — it just targets a different pinout or
flash geometry than the chip actually on the board.

**Example:** usb_io_fw commit `19203e6` ("fix: set RP235xB (QFN-80), set correct LED pin") changed
`b200_usb_io/Cargo.toml`'s `embassy-rp` feature from `rp235xa` to `rp235xb` — the board uses the QFN-80 package,
not the package the `rp235xa` feature assumes — alongside a GPIO fix for the status LED pin that depended on
the same mismatch.

**Fix:** read the chip feature straight from the board's part number (the template's `chip-info` / board preset
already does this for generated firmware); when hand-editing a HAL dependency's features after a board
respin, cross-check the package suffix against the schematic's MCU designator, not against what built last time.

**Detection:** check: cfg mid — needs the board record (part number, package) cross-referenced against the HAL
crate's selected feature; a lint can flag a feature that doesn't match the `chip:` the firmware's own board data
names.

## <a id="cfg-6"></a>`CFG-6` — thumbv6m without `portable-atomic` + `critical-section`

Cortex-M0/M0+ (`thumbv6m`) has no compare-and-swap instruction. Crates that need CAS for a lock-free `static`
(`static_cell`'s `StaticCell`, and others) either don't build on thumbv6m at all, or silently need the caller to
turn on `portable-atomic`'s `critical-section` backend (a CAS emulated with a critical section) — and a
workspace that forgets that feature either fails to build for the real target, or links in a `portable-atomic`
that panics at the first CAS.

**Example:** mcan's P2 (G0 support) row: the crate "builds on thumbv7em only" by default, and "on the real
target (thumbv6m), `static_cell` needs CAS, so it only works if the user enables
`portable-atomic/critical-section`" — `g0,embassy` builds were broken until that feature was wired through
(fixed alongside mcan's I1).

**Fix:** any workspace targeting an M0/M0+ part that depends on a crate needing CAS turns on
`portable-atomic/critical-section` (or the crate's own re-export of it) explicitly in that target's feature set;
don't rely on another crate in the tree happening to enable it.

**Detection:** check: cfg easy, chip: M0+ — a lint checks that `portable-atomic` is in the dependency tree with
`critical-section` enabled whenever the target is thumbv6m and a CAS-needing crate (`static_cell`, ...) is
present.

## <a id="cfg-7"></a>`CFG-7` — a dirty build flashed without a mark

A build made from a working tree with uncommitted changes (a quick local fix, a debug print left in) looks
identical to a clean release build once it's running, unless the firmware itself carries and reports a dirty
flag — otherwise "what's actually on this board" becomes a guess from memory.

**Example:** this repo's own `bedrock_build_info::BedrockBuildInfo` carries a `dirty: bool` field alongside the
git SHA (BLD-1), embedded at build time (BLD-2) and readable back off the device; the Notion firmware checklist
item this check is imported from asks for exactly this (a dirty build must say so, not just report a SHA that
looks like a clean commit).

**Fix:** always embed build info with a dirty flag derived from `git status --porcelain` at build time (the
pattern the CLI `--version` convention in AGENTS.md already uses: SHA plus `-dirty`), and have host tooling
treat a dirty build as worth flagging, not silently trusted the way a clean-SHA build is.

**Detection:** check: cfg easy — a lint checks that a firmware depending on `bedrock_build`/`bedrock_build_info`
actually turns on the feature that embeds this (CFG-7 catches `--build-info` left off as much as a missing dirty
bit).

## <a id="cfg-8"></a>`CFG-8` — git consumers not built

A dependency taken by `path` locally (the sibling checkout, fast iteration) can build clean while the same
dependency taken by `git` — what a CI runner or a fresh clone actually gets, since `[patch]` only applies inside
the checkout that declares it — fails, because the API crate it depends on has no `Cargo.lock` of its own, or a
`git` fetch pulls a different revision than the pinned sibling.

**Example:** this repo's TPL-43 found that `--wire-weaver --ww-src git` generated an API crate with no
`Cargo.lock`, which built fine against the sibling path but failed once consumed from git; BLD-8 is the
underlying convention this guards (the workspace takes WireWeaver by git at the root, local work patches to the
sibling `../wire_weaver`, and *only* builds run inside that checkout see the patch) — a CI job or another PC's
checkout that never applied the patch is exactly the "git consumer" this check means.

**Fix:** build (or at least `cargo check`) the git-sourced form of every dependency that's normally taken by
path during local work, not only the patched path form; commit a `Cargo.lock` for any generated crate meant to
be consumed standalone.

**Detection:** check: cfg mid — CI running outside the workspace's own `.cargo/config.toml` patch (a fresh
clone, or `cargo package`'s isolated build) is the natural place this surfaces; a lint can at least check that a
generated API crate ships a `Cargo.lock`.

## <a id="cfg-9"></a>`CFG-9` — a committed path dependency or `[patch]` to a sibling checkout

A dependency taken by `path = "../wire_weaver/..."`, or a committed `.cargo/config.toml` / `[patch]` that redirects
a git source to the sibling checkout, has no commit in `Cargo.lock`: the lock records whatever that sibling held
when it was last written. The same commit of the repo then builds different code on different PCs, and a `--locked`
lint or test passes on one PC and fails on the next, depending only on which commit that PC's sibling is at.

**Example:** on 3 Nov 2026 embedded_bedrock's land gate built with the sibling wire_weaver at `c440491e` while the
lock was made against `a39ddd9`, and another PC's older sibling took exact_units at git HEAD instead of the pinned
revision, so `--locked` failed there too (BLD-10, P2528#3nov-1235); vb135_fdcan_iso_usb_fw 0.2.8 hit the same.

**Fix:** committed manifests take such crates by git only and `Cargo.lock` pins the commit; bump with
`cargo update -p CRATE` and commit the lock. Keep co-development explicit and uncommitted: a gitignored
`.cargo/config.toml` made from a committed template (`just dev-local`, `just dev-git`), never a committed patch.

**Detection:** check: cfg easy — a lint over every `Cargo.toml` and committed `.cargo/config.toml` flags a `path`
that leaves the repository (`../`) and any `[patch]` entry with a `path` outside it; a clean clone with no siblings
running the `--locked` recipes is the end-to-end test.
