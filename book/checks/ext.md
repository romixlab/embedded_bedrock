# 🔌 EXT — External chips and buses

Detail for the `EXT` checks of the [check catalogue](../checks.md).

## <a id="ext-1"></a>`EXT-1` — a NAK is not "the chip is gone"

Many I2C and SMBus devices NAK while they are busy (post-reset init, an internal conversion, a hub attaching to
its upstream port): a single NAK is not evidence the chip is absent or dead. Code that tears down and rebuilds
state on the first NAK churns on perfectly healthy hardware and can mask the one case that really matters: the
chip woke up with its configuration gone.

**Example:** typec_dongle's hub watchdog (FW-18) originally probed the hub with a zero-length I2C write and
treated the NAK as "the hub reset" — the hub NAKs for 200-600 ms after every boot while it attaches and while
the host enumerates it, so the probe reset the hub one or two times on every single boot even though nothing
had gone wrong there (fixed by FW-41, which changed the probe to the signature below).

**Fix:** use a positive signature instead of the absence of a NAK: read an ID or status register and require it
to actually say "freshly reset". FW-18's fix reads the hub's VID/PID back and only recovers when it comes back
all-zero *twice* in a row, which is what a reset hub reports before it re-attaches, and adds a hold-off after an
attach so the probe doesn't race the chip's own bring-up (no probing for 1 s after an attach, and a request
within 2 s of one is the echo of our own reset, not a new loss).

**Detection:** review — telling a real loss from a busy NAK needs the chip's datasheet and the actual failure
mode on the board, not a pattern a linter can match.

## <a id="ext-2"></a>`EXT-2` — a sagged rail resets the chip but not the MCU

A chip on its own supply rail (or one shared with something that draws a big inrush) can brown out and lose its
configuration — registers back at their reset values — while the MCU, on a stiffer rail or with its own
decoupling, keeps running. Without a watchdog that notices this, the firmware keeps talking to a chip it thinks
is still configured, and every request after the brown-out behaves as if the chip were simply broken.

**Example:** typec_dongle's hub (U301) shares `+3V3_DCDC` with the MCU; a hard turn-on load on the header's
switched VBUS (inrush into a large downstream capacitor) can sag that rail enough to reset the hub without
resetting the MCU, silently losing the hub's SMBus configuration and USB attach state (FW-18). The firmware's
`watchdog_task` catches it from either side: the composite USB device (carried on the hub's port 1) dropping out
of the configured state it had reached, or the periodic I2C probe from EXT-1 reading the hub's VID/PID back
all-zero twice.

**Fix:** a periodic positive-signature probe (EXT-1) plus a USB/bus-side symptom gives two independent signals
of the same loss; on either one, redo the chip's reset, config and bring-up sequence rather than waiting for the
next full MCU reset to fix it.

**Detection:** review — needs the schematic's power tree to know which externals share a rail with what, and
which of those rails can sag independently of the MCU's own supply.

## <a id="ext-3"></a>`EXT-3` — reading "who are you" before the chip has a config

Some chips' ID registers only read a trustworthy value once the chip has been configured; before that they can
hold whatever was last written (no power-on-reset default) or sit at zero. Code that identifies a chip or a board
variant by reading those registers *before* configuring anything gets a coin flip instead of an answer.

**Example:** typec_dongle tried to read the USB hub's own power-on-default VID/PID/DID to detect the board
variant before configuring it, and it read back all-zero regardless of the delay tried (0/5/100/1000 ms) — the
hub's ID registers simply hold whatever was last written there on this part, confirming a caution already on
record for the board (FW-22). The working signal was a hardware strap instead: PA10 pulled low only on the
"Plus" variant by a resistor fitted on that assembly, read against the MCU's own pull-up.

**Fix:** identify a chip, revision or board variant from something that exists independently of the chip's own
configuration state — a strap, a resistor-coded pin, a part number read off a different register that *is*
documented to hold a factory value — not from "ask the chip who it is" when the chip hasn't been told anything
yet.

**Detection:** review — the datasheet's register description is the only way to know whether an ID register has
a real factory default on this part.

## <a id="ext-4"></a>`EXT-4` — read-modify-write over a bus, and reading clear-on-read twice

Read-modify-write of an external chip's register over I2C/SPI is two bus transactions with a window between
them: another path (an interrupt-driven update, a concurrent caller) can change the chip's state in that window,
and the write-back clobbers it. Reading a clear-on-read status or event register more than once for the same
event silently drops the second reader's view of it — the first read already cleared what the second one wanted
to see.

**Example:** pd_bench's `fusb302` driver states its rule in `AGENTS.md`: "No read-modify-write: registers are
written from shadow copies; clear-on-read registers are read once." The driver keeps its own in-memory shadow of
what each writable register should contain and writes the shadow wholesale, and any clear-on-read register
(interrupt/status flags) is read exactly once per occasion and the value distributed from there, not re-read by
a second interested piece of code.

**Fix:** keep an in-RAM shadow of configuration registers the firmware itself is the only writer of, and update
the shadow then write it whole; for clear-on-read registers, read once into a local value and hand that value to
every consumer instead of letting more than one place touch the chip.

**Detection:** src mid — a lint can flag a write immediately preceded by a read of the same register/field
without a shadow struct in scope, and a second read of a known clear-on-read register within one logical
operation.

## <a id="ext-5"></a>`EXT-5` — a sense line that isn't wired the way the code assumes

A sense pin (card detect, VBUS presence, a PHY's "power good") can be absent, tied the wrong way, or simply not
connected to what the code expects. Code written against the chip's datasheet or a reference design, without
checking the actual board, inherits whatever that reference assumed instead of what is really on the net.

**Example:** the ZY12PDN reference board ties the FUSB302B's VBUS-sense pin straight to GND, so `VBUSOK` never
asserts on it; a sink driver that only attaches on `VBUSOK` (per FUSB-2) never attaches at all on that board and
needs attach/detach decided from CC alone (BC_LVL/COMP with the PD-layer debounce times) as a config option.
Card-detect polarity is the same kind of trap: on the rePort rev B SD slot, PG4 reads high with a card in and
there are 10K pull-ups to 3V3 on CMD/D0-3/DET but none on CK (SD-3) — getting the sense polarity backwards there
made every SD command time out with no card present to explain why.

**Fix:** read the connector/chip datasheet and the actual schematic net, not a reference design's assumption,
and make the sense behavior configurable per board rather than hardcoded when more than one wiring exists.

**Detection:** review — this is a schematic-reading step, not something source analysis can check without a
board model.

## <a id="ext-6"></a>`EXT-6` — one path skips the settle time the others respect

A settle or start-up time (VBUS ramping after a hard reset, a reset pulse to the first command the chip will
accept, power-on to the first register access) is easy to get right on the common path and easy to miss on a
less-exercised one — a fast retry loop, an error-recovery branch, a path only a specific host driver takes.

**Fix:** give the settle time its own named constant tied to the datasheet figure it comes from, and make every
path that can reach the chip after a reset or power-up go through the same wait instead of each caller re-deriving
(or forgetting) it.

**Detection:** review — the datasheet gives the number; whether every path actually observes it needs reading
the call graph, which stays a review item until the lint engine can model timing.

## <a id="ext-7"></a>`EXT-7` — an encoded field without a range check

A register field that encodes a user value as `value - 1` (or similar) underflows silently to a huge number, or
truncates and masks to 0, when the caller passes an out-of-range value (0, or something bigger than the field's
width). Nothing crashes; the chip is configured with a value nobody intended.

**Example:** `bedrock_can`'s bit-timing types encode this correctly as the model: `mcan`'s `NominalBitTiming::new`
/ `DataBitTiming::new` are `const fn` that validate the RM0468/RM0444 ranges before ever touching a register —
nominal prescaler 1..=512, seg1 2..=256, seg2 1..=128, SJW 1..=128 (data: 1..=32, 1..=32, 1..=16, 1..=16), plus
SJW <= seg2 — and return a `BitTimingError` instead of silently writing a bad `value - 1` into NBTP/DBTP; the
register encoding itself is a separately host-tested pure function (mcan T1).

**Fix:** validate the caller's value against the field's real range before the `- 1` (or other) encoding, and
fail with a typed error rather than letting the subtraction wrap.

**Detection:** src mid — a lint can flag an unchecked arithmetic encode feeding a register write, though proving
the range is actually enforced needs the call graph back to the public API.

## <a id="ext-8"></a>`EXT-8` — a config value that never reaches the hardware, or reaches it once and goes stale

Two related failures: a setter that updates in-memory config but the apply path never actually writes it to the
peripheral, so the config "takes" in the API but not on the wire; and a value that *was* written once but gets
re-applied stale after some other mode change resets or re-derives it, because nothing re-sends it on that path.

**Example:** `mcan`'s own history has both. The global filter (F1) had exactly the first failure: `set_global_filter`
updated the driver's config struct but didn't store the filter for re-application, so the next mode transition
(Config → Normal, say) silently re-applied the *old* filter — found by a HIL test (`loopback::global_filter_routes_to_fifo1`)
that would otherwise have passed by luck. Transceiver delay compensation (T2) and the timestamp counter (S1) show
the second shape: `set_timestamp_counter_source` was simply never called from `apply_config` at all, and the
timestamp prescaler wrote the divisor itself instead of `divisor - 1` into TSCC — a config that looked applied
but wasn't, or was applied wrong every time.

**Fix:** every setter that should take effect on real hardware needs a test that configures it, changes mode or
re-applies config, and then reads the value back from the peripheral — not just from the driver's own shadow —
to prove the write actually happened and survived the transition.

**Detection:** test — this is exactly what a HIL or host-model round trip (set → transition → read back) catches;
`check: test` is already the right tier in the catalogue for all three (F1, T2, S1).

## <a id="ext-9"></a>`EXT-9` — one instance's init zeroes memory another instance owns

A shared peripheral memory region (message RAM, a register file split by instance) zeroed "whole" by one
instance's own initialization clobbers whatever a second instance already wrote there, if the hardware doesn't
actually give each instance an isolated region the way the API pretends.

**Example:** `mcan`'s message-RAM handling (R4) has to zero only the instance's *own* region: on H7, `set_layout`
zeroes just the layout's region when the layout changes, which matters because re-applying the same layout on a
mode transition must not wipe filters or TX buffers another instance already configured; on the "lite" cores
(G0), entering Config mode zeroes the instance's fixed 212-word block at its own offset (FDCAN2 at +0x350),
not the whole `FDCAN_MSGRAM_LEN_WORDS = 512` region shared with FDCAN1. This still wants a two-instance HIL test
(mcan Q5) to prove instance 2's zero never touches instance 1's region.

**Fix:** know the exact byte range each instance's own zero/init touches (from the register map, not from "the
whole peripheral"), and test it with both instances configured, one re-initializing while the other holds state.

**Detection:** review — needs the chip's message-RAM layout table per instance; a lint could check a zero-fill
range against a chip-data table once that table exists (currently `check: review` in the catalogue).

## <a id="ext-10"></a>`EXT-10` — another IP variant's register map, with its own reserved bits

The "same" peripheral across chip families is sometimes genuinely a different IP variant with a different
register map and different reserved-bit placement, not just a different base address. Code written against one
variant's reference manual and ported by just changing addresses carries over masks that are wrong for the
other variant's reserved bits.

**Example:** `mcan`'s G0 support (P2) targets a different FDCAN IP ("lite") than H7's, with its own reduced
register map (`pac::fdcan_v1`): the lite core is still missing RXGFC LSS/LSE/F0OM/F1OM, has no
`MessageRamLayout` (a fixed layout instead), and groups ILS differently (a source moves its whole interrupt-line
group, not an individual bit) — porting H7 masks onto it without checking RM0444 against RM0468 would silently
hit reserved bits that mean something else on this variant.

**Fix:** pull the register map and reserved-bit layout for *this* variant from *its* reference manual, even when
the peripheral has the same name and mostly the same API shape on another chip in the family.

**Detection:** review — chip-table-driven src checks could flag a mask constant shared between two chip features
with different register maps, but today this is a manual reference-manual comparison (`check: review`).

## <a id="ext-11"></a>`EXT-11` — a HAL driver taken on trust

A HAL crate's driver for an external chip was written for *some* use case; it may not meet what this product
needs on timing accuracy, error recovery, or even basic robustness (a `new()` that panics on a condition the
product can hit in the field). Pulling it in without checking it against the product's own requirements inherits
whatever gaps the HAL author didn't need to close.

**Example:** rePort's I2C design note (`i2c-master-driver.md`) exists specifically because embassy-rp's I2C
driver was evaluated against what the product actually needs — NAK behavior and index semantics, recovery from
a stuck bus, what panics are reachable — rather than assumed fine because it's the "official" HAL driver for the
chip.

**Fix:** write down, per external chip, what the product actually needs from the driver (timing, error paths,
panic surface) and check a candidate HAL driver against that list before taking it, the way the rePort I2C note
does; budget for a thin wrapper or a fork when the gap matters.

**Detection:** review — this is a one-time evaluation per driver choice, not something an automatic check can do.

## <a id="ext-12"></a>`EXT-12` — talking to a peripheral before its clock is confirmed on

Enabling a peripheral's RCC clock and then immediately touching its registers can run ahead of the enable
actually taking effect on some STM32 parts — there can be a few cycles of latency between the write to the RCC
enable bit and the peripheral's registers becoming live, and an access in that window reads or writes garbage
(or faults, depending on the peripheral and bus).

**Fix:** read the RCC enable bit back after setting it, before the peripheral's first register access, so the
write has actually landed; embassy's own peripheral constructors generally do this, but a hand-rolled register
poke (an errata workaround, a peripheral the HAL doesn't own) needs to do it explicitly.

**Detection:** src mid — a lint can look for a raw register write to a newly-enabled peripheral with no
intervening read-back of its RCC enable bit, chip: STM32.
