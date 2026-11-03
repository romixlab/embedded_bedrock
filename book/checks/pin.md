# 📌 PIN — Pins and IO

Detail for the `PIN` checks of the [check catalogue](../checks.md).

## <a id="pin-1"></a>`PIN-1` — Keep the pin driver, don't drop it

embassy's GPIO wrappers (`Output`, `Input`, `Flex`) disconnect the pin's function on `Drop`: `let _ =
Output::new(..)` binds to the wildcard pattern, which drops the value at the end of the statement, and the pin
goes back to its reset state (usually a floating input) right after it was configured. The firmware looks like it
drove the pin, because the register write happened, but the driver object that was supposed to hold that
configuration for the firmware's life never lived past the line that created it.

**Example:** donguru_template's `firmware/src/main.rs` shows the pattern to follow instead: `let _pa4 =
Output::new(p.PA4, Level::High, Speed::Low)` binds the driver to a named (if underscore-prefixed) local that lives
for the rest of `main`, so the pin stays driven. The generic Notion firmware checklist item on "destructors" is
the same lesson in HAL-agnostic form: anything whose `Drop` undoes a hardware configuration needs an owner that
outlives the configuration's intended lifetime.

**Fix:** bind every pin driver that must hold its configuration to a named variable (or a field of a struct kept
in a task or a `static`) that lives as long as the configuration must, never to `_`. Where a pin genuinely is
meant to be released, that release should be an explicit, visible call, not an implicit one from a temporary
falling out of scope.

**Detection:** check: src easy — `let _ = <ctor>(..)` (or an unnamed temporary passed by value and never bound)
around a HAL GPIO constructor is a syntactic pattern `syn` can flag without needing the firmware model.

## <a id="pin-2"></a>`PIN-2` — Safe state before anything slow

The first instructions after the HAL/clock init run with every pin still at its hardware reset state, which on a
board with output-enable lines, shift-register chains or power switches is not necessarily a safe state — some
of those lines may default to driving something on, or to an undefined level that a downstream chain latches
into an unsafe pattern. Anything that takes time before the firmware gets to set pins deliberately (a slow USB
stack bring-up, waiting on a sensor) is time spent with the board's outputs in whatever state the silicon reset
left them.

**Example:** usb_io_fw's B200A board driver (`PLAT-9`) has a 32-bit 595 shift-register chain and interlocks (one
VREF source, one I2C path, pull-ups only with their path, one SPI translator); `src/chain.rs` loads the chain's
safe pattern as the very first thing after HAL init, before USB starts, specifically because a chain left at its
reset pattern could short `VBUS` into `+3V3` through the board's power switches if USB came up first and a host
started drawing power before the chain was in a known state.

**Fix:** write the board's safe pattern (chain outputs, power-switch enables, anything with an interlock) before
starting USB, before any slow peripheral bring-up, and before anything that can take an unbounded time to
complete; keep that write as the first board-specific code after the HAL/clock init, not after some other
feature's setup.

**Detection:** check: src mid (finding "the first board-specific write after HAL init" needs the firmware's call
graph, not just a text match) + review (whether the pattern it writes is actually the board's safe one is a
hardware question).

## <a id="pin-3"></a>`PIN-3` — Translators and bus switches: on last, off first

A level translator or a bus switch bridges two sides that may be at different, unrelated states; turning it on
while either side isn't at the level or the configuration it expects can glitch both sides at once — usually a
spurious edge the other side's peripheral latches as if it were real activity. The fix shape is always the same
(configure both sides, release pins to their intended level, then enable the bridge; disable the bridge before
either side changes), but getting every enable line in the right order on real hardware is easy to get subtly
wrong when more than one signal shares the same gate.

**Example:** usb_io_fw's I2C bring-up (`IF-2`) established the bring-up order (pull-ups and pin levels settled
before the translator's output-enable). `IF-17` found a residual glitch even with that order followed: on the
B200A, the chain bit `EX30` is both the TXS0102's own OE *and*, through a transistor, the gate of a separate bus
switch feeding the translator's external-side pull-up node — so enabling the translator and powering its
pull-up node happen on the same edge, and the translator passes the pull-up node's low "not yet charged" state
through to the MCU side for a few microseconds before the 10k pull-ups there catch up, latching a spurious edge
pair on both I2C lines.

**Fix:** give the bus switch and the translator's own enable independent chain bits where the hardware allows
it (a rev B fix tracked under IF-17: switch the bus switch on first, then the translator's OE, or add a pull-up
directly on the translator's external-side node so it doesn't depend on the switch at all); where one gate must
drive both, budget for the glitch on the side that can tolerate it and keep it off the side that can't.

**Detection:** check: test (catching the edge pair needs a GPIO interrupt-latch read or a scope, not a static
read of the source) + review (whether a chain bit is shared between two logically distinct enables is a
schematic question).

## <a id="pin-4"></a>`PIN-4` — Keep FUNCSEL, override with OEOVER/OUTOVER

On RP2040/RP2350, a peripheral block reads the logical OR of every pin whose `FUNCSEL` names that peripheral:
moving a pin to `GPIO`'s plain function and back to hand control to software for a moment (a bit-banged
recovery sequence, a manual bus reset) is a function-select change the peripheral itself sees as a bus event —
an I2C block watching a start/stop pattern on SDA/SCL can see a spurious start with no matching stop, and gets
confused about the bus state from then on.

**Example:** `bedrock_can`'s own notes and i2c_periph's DW_apb_i2c master driver (`BE-1`) both point at the same
RP2040/RP2350 property: "on an RP2350 the pins keep the I2C function and software drives them through
`GPIOn_CTRL.OEOVER` / `OUTOVER`: the chip gates a peripheral's input by function select, so a function change is
a bus event to the block" — the fix is to never reassign `FUNCSEL` away from the peripheral just to bit-bang a
recovery, and instead override the pad's output enable and output value directly while `FUNCSEL` stays pointed
at the peripheral the whole time.

**Fix:** use `GPIOn_CTRL.OEOVER`/`OUTOVER` (force output enable / output value regardless of what the peripheral
would otherwise drive) for any software-driven excursion on a pin a peripheral owns, and leave `FUNCSEL` alone.

**Detection:** check: src mid · chip: RP — needs recognizing a `FUNCSEL` write to a pin that's also configured
for a peripheral elsewhere in the firmware model, not just a single-line pattern.

## <a id="pin-5"></a>`PIN-5` — Don't rely on a pull the BOM might not fit

A pin whose correct level depends on an external pull-up or pull-down that's a DNP (do-not-populate) option on
the schematic works fine on a board where that option happens to be fitted, and floats — or floats to whatever
the pad's own weak default pull does — on every board where it isn't. The firmware can't see "is R301 actually
soldered down", so if it also leaves the pad's internal pull off (assuming the external one will do the job),
the result depends entirely on a BOM decision the firmware has no way to check.

**Example:** usb_io_fw's I2C lines (`IF-14`) floated low right after boot on the B200A because the schematic's
MCU-side 3k3 pull-ups (R301/R302) are DNP on that board revision (the hardware doc had listed them as fitted —
itself wrong), and the firmware had turned off the pad's own pull assuming the external one would hold the line;
with both translator paths off, nothing held the lines, and `is_scl_high`/`is_sda_high` answered false while the
connector side genuinely sensed 3.2 V.

**Fix:** turn on the pad's own internal pull whenever an external one is optional or unverified, even if a
later hardware revision adds the external pull too (two pulls on the same rail in the same direction are
harmless; no pull at all is not).

**Detection:** check: review — this needs cross-checking the schematic's DNP list against what the firmware
assumes, which isn't visible from the source alone.

## <a id="pin-6"></a>`PIN-6` — One owner per pin

A pin that two independent pieces of code can both claim (a diagnostic/test API meant for bring-up, and the
driver task that normally owns that pin in production) will have whichever one wrote last win, silently, with
no indication to either side that the other also has an opinion about that pin. The failure mode is specific to
whichever two features happen to collide, and looks like an intermittent hardware fault rather than two owners
stepping on each other.

**Example:** usb_io_fw's pin-diagnostic feature (`IF-16`) could still take GPIO8/GPIO9 as generic diagnostic
points (`b200_board::pin_diag::MCU_PINS`, every pin `BoardState::available` lists) even though the I2C master
task owns those same two pins whenever it's running; a `diag.pins` request could put them under SIO control out
from under a live I2C transaction, and the I2C task would only reclaim them on its own next configure — bus
recovers, but by luck of timing, not by design.

**Fix:** track pin ownership centrally (one registry that every feature checks before taking a pin) and refuse
a request for a pin another feature currently owns, with an error the host can see, instead of letting the last
writer win.

**Detection:** check: src hard — proving two independent code paths can reach the same pin concurrently needs the
full firmware model (every task, every API handler, what each one can touch), not a local pattern.

## <a id="pin-7"></a>`PIN-7` — Stop driving before you turn the direction pin

A level translator's direction pin (controlling which side is the source) must change while neither side is
actively driving the line, or the moment of the flip itself glitches whichever side just lost control — the old
driver may still be asserting a level for a few nanoseconds after the direction pin says the other side is now
the source, and both sides briefly fight.

**Example:** usb_io_fw's GPIO slot (`IO-1`) does this correctly by construction: `frontend::apply` first pulls
the IO pad towards the level it's about to take, then turns `DIR`, and does both as one atomic SIO register
write — the pad is never asserting the old direction's level at the instant `DIR` flips, because the write that
changes the level and the write that changes direction aren't two separate, orderable operations the hardware
could interleave.

**Fix:** stop driving (or settle the pad to the level the new direction expects) and flip the direction pin as
close to atomically as the hardware allows — one register write touching both, where the peripheral supports it,
rather than two writes with a gap between them.

**Detection:** check: test — this is a timing property at the pin, only visible with a scope or a logic analyzer
across the direction transition; a register read after the fact can't show what happened during the flip.

## <a id="pin-8"></a>`PIN-8` — Unused pins need a defined state

A pin left unconfigured defaults to a floating input on most parts, and a floating input near a digital
threshold draws current, radiates and picks up noise that can register as spurious edge events if anything
(an interrupt, a wake source) is watching it. "Unused" is not the same as "doesn't need a decision": every pin on
the package needs either an analog/disconnected state (STM32's `Analog` mode on an unused pin removes the input
buffer entirely) or an explicit, deliberate pull.

**Fix:** set every pin not used by the design to `Analog` (STM32: no input buffer, no leakage) or to a defined
pull if the part has no true disconnect mode, as part of the same pass that configures the pins that are used —
not left to whatever the reset default happens to be.

**Detection:** check: src mid — needs the chip's full pin list and the firmware's pin-configuration coverage to
say which pins were never touched, which is a cross-reference against chip data, not a single-file pattern.

## <a id="pin-9"></a>`PIN-9` — Pin maps belong to the board record, not a build feature

Choosing a board variant's pin map with a `cfg` feature means the binary itself only knows how to drive one
variant's wiring; flash the wrong variant's build onto a board and every pin the two variants disagree about
gets driven as if it were the other board — a feature that's merely absent on one variant can become a feature
actively driving current into whatever *is* there on that variant instead.

**Example:** rePort's SD-card design (P2424 `sd_card.md`, board record in flash, PLAT-15) chose the opposite of
a build-time feature: the pin map (whether SD lines exist and what they're wired to, `SdPinout`) is read from a
board record in flash/OTP at run time, so a single binary is safe to flash onto every board — a board with no
SD rework and a blank record defaults to `SdPinout::B200A` (as built) and keeps its SD-capable lines as plain
inputs, answering `NoMedium` regardless of which build is on it, so "the wrong build on the wrong board" simply
can't drive a line the physical board doesn't have wired that way.

**Fix:** read the pin map from a board record (flash, OTP, or at minimum a strap read at boot) rather than from
a cargo feature baked into the binary, and default unconfigured/unknown boards to every variant pin set as a
safe input, never as an assumed output.

**Detection:** check: src mid — recognizing "this pin map comes from a `cfg`-gated constant, not a runtime read"
needs the firmware model to trace where the pin table's values originate.

## <a id="pin-10"></a>`PIN-10` — Release to Hi-Z at every session end, and don't back-feed

A line driven onto an external device that may be unpowered can back-feed current into that device through its
own protection diodes; the firmware needs to release every such line to a high-impedance state whenever the
session using it ends, including the unclean endings (a USB disconnect or reset the host didn't negotiate, a
suspend, a cancelled transfer), not only the clean "the host asked to disconnect" path.

**Example:** typec_dongle's plug CC drive (`FW-17`) releases PD0/PD2 to Hi-Z together with SWCLK/SWDIO on
`DAP_Disconnect`, on CMSIS-DAP v2 endpoint disable, *and* on `sbu.release` — three different ways a session can
end, all wired to the same release path, specifically because a downstream board gating its own SBU-to-SWD mux
on the plug's CC level needs that line to go away cleanly whichever way the session stopped. The planned PD
source role (`API-30`) carries the same obligation forward: the plug stays off unless the active mode itself
switches it, and every mode's limits are enforced so a plug that's sourcing power doesn't keep sourcing it past
the session that asked for it.

**Fix:** enumerate every path that can end a session that drives an external-facing line (clean disconnect,
unclean disconnect, USB reset, suspend, host crash/timeout, an explicit cancel) and route all of them through
one release function, rather than adding the release call only to the path that was tested.

**Detection:** check: review — finding every session-ending path in a USB/host-facing state machine is a
control-flow question that needs reading the state machine, not a pattern match.

## <a id="pin-11"></a>`PIN-11` — Check the board record before using a rework-gated feature

A hardware feature that only exists after a specific rework (a bodge wire, a resistor change, an LDO fitted on
some boards and not others) must not be used just because the firmware *build* supports it: the firmware also
has to check, at run time, that *this particular board* actually has the rework, or it ends up asserting a
feature the board can't physically deliver.

**Example:** rePort's board-record design (P2424 `api_design.md`) gates exactly this: "1.8 V" is refused unless
`HwFeatures::ldo_1v8` in the OTP board record says that board's LDO rework is fitted (on rev A as built, that
level is actually 3.2 V, not 1.8 V, until rework 3), and the analog rail answers `Unavailable`/`NoPower` unless
the record says rework 1 (the `+5V0_FLT` supply) is present — the API refuses the request with a typed reason
rather than silently producing a wrong voltage or applying a non-existent supply.

**Fix:** gate every rework-dependent feature on a board record read at boot (OTP, flash, or at minimum a strap),
not on the build configuration, and answer host requests for a missing rework with an explicit "not available on
this board" rather than attempting it.

**Detection:** check: review — whether a given code path checked the board record before using a rework-gated
feature needs reading that path against the hardware document, not a pattern a lint can see reliably.

## <a id="pin-12"></a>`PIN-12` — Set the pin direction on every path that uses it, not only from a command

A pin whose direction (input vs. output) is normally set by a protocol command can stay at its boot-time
direction on any code path that reaches the pin without going through that command first — including a path
added later that assumed the command had already run.

**Example:** typec_dongle 0.3.0 had exactly this gap on SWCLK: the pin was only ever driven as an output by
`DAP_SWJ_Pins`, which probe-rs's CMSIS-DAP v2 driver never sends, so SWCLK stayed an input on every real session
and no target ever acknowledged a clock edge (tracked under FW-15/FW-20, the SWD speed work) — the direction was
correct in the one code path that was tested by hand, and wrong on the one real hosts actually use.

**Fix:** set the required direction unconditionally on every entry point that drives the pin (session start, the
first clock edge, a reconnect), rather than relying on a command that establishes the direction as a side effect
and assuming every caller sends it first.

**Detection:** check: review — this needs checking every code path that touches the pin against what direction
it assumes, which is a control-flow question a simple pattern can't resolve.

## <a id="pin-13"></a>`PIN-13` — A pin driven by GPIO doesn't survive an MCU reset

A GPIO output goes back to its hardware reset state (almost always a floating or pulled input) on every MCU
reset, including a reset the firmware itself triggers (a watchdog, a software reset, a crash recovery). A signal
that genuinely needs to hold its level *across* a reset — because downstream hardware latches on it, or because
losing it mid-transaction is unsafe — can't be a plain GPIO pin; it needs something the reset doesn't touch.

**Example:** typec_dongle hit this twice on the same board: `FW-46` found that pulsing a reset line as a plain
GPIO dropped a downstream hub that needed to *stay* enumerated through the MCU's own reset, so the fix was to
stop pulsing it and instead keep the hub's enable held through reset; `FW-49` found the plug's VBUS enable
(`PWR_EN` on PA6) has an external pull-down (R601), so plain GPIO drive means plug VBUS — and whatever DUT is
drawing power from it — drops at *every* MCU reset and stays off through the firmware's own ~43 s ROM update,
even though the MCU's reset has nothing to do with whether the DUT should still have power.

**Fix:** move a signal that must survive reset into the backup domain (an `RTC_OUT` output, or a backup register
driving external logic) where the part has one, or pick a pull direction on the external hardware that makes the
GPIO's reset state the signal's *safe* state, since the GPIO reset state can't be changed.

**Detection:** check: review — whether a given signal's "must survive reset" requirement is met by its current
pin type is a cross-check against the schematic and the reset behavior, not something visible in the source
alone.

## <a id="pin-14"></a>`PIN-14` — Sample noisy inputs more than once

A single read of a digital input can catch the input mid-transition, or catch noise that briefly crosses the
logic threshold without a real edge having happened; anywhere a human or an uncontrolled external signal can
make the pin bounce (a mechanical button, a cable connect event), one sample is one chance to be fooled by a
glitch that looks exactly like a real edge for one read's duration.

**Fix:** sample the pin several times over a short, bounded window and require the result to be stable (or use
an edge-detect-then-confirm pattern) before treating a transition as real; the exact window depends on the
source of noise (button bounce is milliseconds, EMC-induced glitches are much shorter), and should be measured
for the specific input, not guessed.

**Detection:** check: review — whether an input is debounced is a property of the surrounding code (is this read
part of a filtering sequence or not), which needs reading the call site, not a pattern that's reliable on its
own.

## <a id="pin-15"></a>`PIN-15` — Match drive strength to what the line actually needs

A pad's drive strength and slew rate are usually configurable (weak/normal/high current, slow/fast edges), and
picking "fast and strong" by default because it's available costs more than it buys: faster edges radiate more
EMC energy for a given trace, and higher drive current isn't free on a bus shared with other drivers. The
tradeoff point depends on the actual frequency and load on that specific line, not on what the pad can do at
its maximum setting.

**Fix:** pick drive strength and slew rate from the line's real frequency, trace length and load (consulting
AN1015-style EMC guidance where the line is a radiation concern), and treat the fastest/strongest setting as
something to justify, not the default.

**Detection:** check: review — this is a signal-integrity trade-off against the schematic and the trace
geometry, not something the firmware source alone can decide.

## <a id="pin-16"></a>`PIN-16` — A dead-battery pull-down can hold a pin even after you release it

STM32G0's UCPD block drives a dead-battery pull-down on its CC/USB pins that's active by default from reset
(so a Type-C dead-battery device still looks like a cable-powered sink before any firmware runs). The pull-down
is released by a specific `SYSCFG` strobe bit, and once that strobe has been set it must *stay* set — clearing
it again re-arms the dead-battery pull-down, so a pin that's being read for something unrelated to UCPD can
still be held low (or see a glitch on each configure) by a pull-down the firmware thought it had already turned
off for good.

**Example:** typec_dongle's board-variant detection (`FW-22`) reads PA10 against the MCU's own ~40k pull-up to
tell Main from Plus hub variants — but PA10 is also a UCPD1 dead-battery pin, so the read only works after
releasing UCPD1's dead-battery pull-down via `SYSCFG.cfgr1().ucpd1_strobe()`, and the firmware never clears that
strobe again afterwards (clearing it would silently re-enable the pull-down and break the read, and every other
use of the pin, until the next reset).

**Fix:** release the dead-battery pull-down on every UCPD-capable pin the firmware repurposes, once, early, and
never write the strobe back to its "re-arm" state afterwards; treat the strobe as a one-way boot-time latch, not
a toggle.

**Detection:** check: src mid · chip: G0 — recognizing "this pin is also a UCPD dead-battery pin" needs the
chip's pin table; recognizing a strobe write that clears what an earlier one set needs the firmware model, not
just a single-line pattern.
