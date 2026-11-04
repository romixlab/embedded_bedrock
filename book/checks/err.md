# 🐞 ERR — Silicon errata

Detail for the `ERR` checks of the [check catalogue](../checks.md). This is the one category where "why" is
almost always "because the document says so" — every section with a real erratum quotes it, per [[ERR-2]] and
our internal firmware rule ("check the errata, always"): never a paraphrase from memory.

## <a id="err-1"></a>`ERR-1` — Read the errata sheet for the silicon on the board, every time

A chip's errata sheet is keyed to a specific silicon revision (ST: `DBGMCU_IDCODE.REV_ID`; RP2040/RP2350:
`CHIP_ID`/bootrom version; Nordic: `FICR.INFO.VARIANT`; Espressif: the chip revision `esptool` reports), and the
revision on a board already in the field is not necessarily the revision whose errata someone remembers from a
previous project. Any work that touches a peripheral, clock tree, flash, DMA, low-power mode or the boot path is
in scope: these are exactly the areas where an erratum workaround, once skipped, shows up as a fault that looks
unrelated to its real cause.

**Fix:** before writing or reviewing code in one of those areas, open the errata sheet for the part *and
revision on the board being targeted* (`bedrock_gen.py chip-info --chip <part>` prints where it is for chips the
template knows), and for STM32 also check Jan Waclawek's [efton.sk/STM32/gotcha](http://www.efton.sk/STM32/gotcha/)
list of reference-manual traps the formal errata sheets don't cover.

**Detection:** check: review — whether the errata sheet was actually read for this change is not something
visible in the diff; it has to be part of the review itself, and our internal firmware rules make it a standing
rule, not a per-task reminder.

## <a id="err-2"></a>`ERR-2` — Quote the erratum next to the workaround

A register poke that exists only to work around a silicon bug reads, to anyone who finds it later without
context, like a mistake — an "off" value, a redundant write, a wait that looks unnecessary. Without the erratum
quoted right there (number, title, affected revisions, the vendor's own workaround text), the next person to
touch that code has no way to tell a deliberate workaround from a leftover experiment, and the two most likely
outcomes are either "helpfully" removing it, or leaving it in place forever on hardware where it no longer
applies.

**Fix:** every erratum-driven register write gets a comment with the document's erratum number, its title, the
silicon revisions it affects, and the vendor's workaround — quoted from the document, not paraphrased — right
next to the code that implements it. `ERR-3` through `ERR-9` on this page are the pattern to follow.

**Detection:** check: src easy — a register write with no comment at all, or a comment that doesn't name an
erratum ID, next to a value that doesn't match the peripheral's documented reset or application value, is a
pattern worth flagging even without understanding the erratum itself.

## <a id="err-3"></a>`ERR-3` — G0 dual-bank flash prefetch across banks

**ES0548 2.2.10** ("Prefetch failure when branching across flash memory banks", STM32G0B1/G0C1; the parallel
sheet for G0C1-only parts is **ES0547 2.2.8**), status **N** (not fixed) on every revision the sheet lists:

> In rare cases, the code prefetch may fail upon branching and function calls across flash memory banks, when
> the device operates in dual-bank mode. The failing prefetch then provides incorrect data to the CPU, which
> corrupts code execution and may lead to HardFault interrupt.

It applies to every 512 KiB part (always dual-bank) and to 256 KiB parts with `DUAL_BANK` set; ST's workaround is
**"None"** — either turn flash prefetch off, or make sure code never branches across the bank boundary.

**Example:** typec_dongle's Donguru firmware (`FW-38`) HardFaulted with garbage in `r0`-`r3` within seconds under
USB load, from a call at `0x0804005c` into `BufReader::read_le32` at `0x08039d18` — a branch from bank 2 into
bank 1 with prefetch on. The template's fix (`TPL-32`) takes ST's second option: prefetch stays off needlessly
costs ~40% throughput (so the template turns it back on for embassy's G0 target, which otherwise leaves it off by
default), and instead `memory.x` gets `ASSERT(__etext <= <bank 2 base>)` on dual-bank G0B1/G0C1, so code that
would reach bank 2 fails to **link**, not to run — the error moves from an intermittent HardFault under load to
a build-time failure every time.

**Fix:** on a dual-bank G0B1/G0C1 target, either leave flash prefetch off, or add a linker assertion that code
never crosses into bank 2, so the erratum's only cost is a build failure instead of a field fault.

**Detection:** check: src mid (recognizing the flash prefetch enable needs the HAL config) + elf easy (the
`ASSERT` in `memory.x` is checked by the linker itself) · chip: G0 dual bank.

## <a id="err-4"></a>`ERR-4` — FDCAN edge filtering and dedicated Tx buffers

**ES0548 2.15.1 / ES0491 2.22.1** ("Desynchronization under specific condition with edge filtering enabled",
G0 / H7), status **A** (workaround available):

> FDCAN may desynchronize and incorrectly receive the first bit of the frame if the edge filtering is enabled
> (the EFBI bit of the FDCAN_CCCR register is set), and the end of the integration phase coincides with a falling
> edge detected on the FDCAN_Rx input pin.

Workaround: **"Disable edge filtering or wait for frame retransmission."**

**ES0548 2.15.2 / ES0491 2.22.2** ("Tx FIFO messages inverted under specific buffer usage and priority setting"),
same status, triggers only when "FDCAN uses both a dedicated Tx buffer and a Tx FIFO (the TFQM bit of the
FDCAN_TXBC register is cleared)"; the sheets' second workaround is **"Use only a Tx FIFO."**

**Example:** vb135's FDCAN bring-up (`CAN-1`, `bedrock_can`) takes both workarounds as the only configuration it
ever builds: edge filtering (`CCCR.EFBI`) stays off, and the TX side is always Tx-FIFO-only (`TFQM` set), never a
dedicated buffer beside a FIFO — there's no runtime switch for either, so the erratum can't be re-introduced by
a later config change.

**Fix:** don't expose edge filtering or "dedicated buffer + FIFO" as configuration options at all on G0/H7 FDCAN;
build the driver so the only reachable configuration is already the workaround.

**Detection:** check: src mid · chip: G0, H7 — needs reading the configuration path to confirm `EFBI` and `TFQM`
can't be set to the erratum-triggering combination, not a single register value.

## <a id="err-5"></a>`ERR-5` — FDCAN DAR without a retransmit path

**ES0491 2.22.3** ("DAR mode transmission failure due to lost arbitration", H7), status A:

> In DAR mode, the transmission may fail due to lost arbitration at the first two identifier bits.

Workaround: **"clear the corresponding Tx buffer transmission request bit TRPx of the FDCAN_TXBRP register and
set the corresponding cancellation finished bit CFx of the FDCAN_TXBCF register, then restart the
transmission."**

**Example:** DAR (one-shot transmission, no automatic hardware retransmit) is `bedrock_can`'s `one_shot` API
option on vb135. The workaround itself — clear TRPx/CFx and restart from software — is **not implemented**: a
frame lost this way under DAR is simply dropped (tracked as `TPL-53` in this repo's FEATURES.md), which is an
accepted gap, not a silent one.

**Fix:** either implement the documented software retransmit for DAR mode, or, if one-shot semantics are only
ever used where an occasional dropped frame at lost arbitration is acceptable, say so explicitly next to the
`one_shot` option and count the drop.

**Detection:** check: src mid · chip: H7 — recognizing "DAR is on and no retransmit path exists" needs the
driver's state machine, not a single line.

## <a id="err-6"></a>`ERR-6` — RP2350 A2 input-enabled pads hold their own leakage high

**RP2350-E9**, Appendix D.5.1 ("Increased leakage current on Bank 0 GPIO when pad input is enabled"), affects
RP2350 **A2** only (fixed in A3):

> When the pad is set as an input (input enable is enabled and output enable is disabled) and the voltage on the
> pad is within the undefined logic region, the leakage current exceeds the standard specified IIN leakage
> level... the pad can source current (typically around 120 μA)... This leakage will hold the pad at around
> 2.2 V... The pad pull-down (if enabled) is significantly weaker than the leakage current in this state and
> therefore isn't strong enough to pull the pad voltage low.

Workaround:

> If pad pull-down behaviour is required, clear the pad input enable in GPIO0.IE (for GPIOs 0 through 47) to
> ensure that the pad pull-down resistor pulls the pad signal low. To read the state of a pad pulled-down GPIO
> from software, enable the input buffer by setting GPIO0.IE immediately before reading, and then re-disable
> immediately afterwards.

**Example:** report_b200a's user button (SW101, GP22, no external pull-down on the board — the firmware relies
on the internal one) hits this exactly: with the input buffer left on between reads, the pin can stay held near
2.2 V (a logic high) after release instead of returning to the pulled-down low. The board's rev A/A4 build
(`TPL-48`'s pin handling, matching `b200_board/src/pin_diag.rs`) enables the input buffer only for the instant of
each read and disables it again immediately after, exactly as the workaround prescribes; Rev B of that board
adds an external pull-down instead, which sidesteps the erratum rather than working around it.

**Fix:** on any RP2350 A2 pad using an internal pull-down as an input, enable `GPIOn.IE` only for the duration of
the read and disable it again right after; don't leave input-enable on between reads. An external pull-down of
8.2 kΩ or less also works per the sheet, and removes the need for the read-time toggle entirely.

**Detection:** check: src mid · chip: RP2350 A2 — needs recognizing a pad configured `Input` + pull-down that
stays input-enabled across more than the read itself.

## <a id="err-7"></a>`ERR-7` — RP2350 ROM reboot: delay must not be zero, and don't set extra FRCE_OFF bits

**RP2350-E19** ("RP2350 reboot hangs if certain bits are set in FRCE_OFF when rebooting"), affects A2 (fixed in
A3 bootrom):

> An incorrect assertion in the boot path assumes all bits (other than FRCE_OFF.PROC1) are clear. These bits can
> only be set during boot if the user had set them and then re-entered the boot path.

Workaround: **"Don't perform a WATCHDOG or POWMAN boot, or a core0 reset with bits other than FRCE_OFF.PROC1
set."**

**RP2350-E30** ("reboot() API fails to reboot for delay_ms == 0"), affects A2, A3 **and** A4 (not fixed in
silicon, fixed only in documentation/software): workaround **"Pass delay_ms = 1 instead. This is handled for you
automatically by the wrapper functions in the Pico SDK as of version 2.3.0."**

**Example:** the firmware template's RP boot module (`app/src/boot.rs.j2`) calls the raw bootrom entry point
`embassy_rp::rom_data::reset_to_usb_boot(0, 0)` directly rather than a Pico-SDK wrapper, so E30's SDK-level fix
doesn't apply automatically: the template passes a 10 ms delay instead of 0, in the comment's own words, "with 0
it doesn't reboot". The same code sets no bits beyond what's needed, which keeps it clear of E19 without needing
a separate workaround.

**Fix:** any direct call to the RP2040/RP2350 bootrom's `reboot()`/`reset_to_usb_boot()` uses a non-zero delay
(the template uses 10 ms), and never sets `FRCE_OFF` bits beyond `PROC1` before a WATCHDOG/POWMAN boot or a core0
reset.

**Detection:** check: src easy · chip: RP2350 — a `delay_ms` (or raw delay argument) literal `0` passed to a
bootrom reboot call is a straightforward pattern match.

## <a id="err-8"></a>`ERR-8` — G0 ADC: wait two kernel clocks around disable, calibration and the calibration factor

efton.sk gotcha 221 ("ADC may freeze around calibration or enable/disable, sequencing has to be observed
carefully"), the G0/C0/WL paragraph:

> In 'G0 (and related 'C0 and 'WL families), there should be a 2 ADC kernel clock delay observed after disabling
> the ADC, and performing any other ADC-kernel-related operation (such as re-enabling ADC, or performing
> calibration, or writing the calibration factor).

The underlying cause (per the same page) is that the ADC is a dual-clock peripheral: the APB side the rest of
the chip talks to and the kernel-clock side that actually runs the conversion need their signals resynchronized
across the clock-domain boundary, and the timing of that resync depends on the exact ratio between the two
clocks — which is why the symptom (an ADC that freezes) is hard to reproduce and depends on the specific clock
configuration in use.

**Example:** typec_dongle's ADC calibration path (`0.20.7`) added exactly this wait after disabling the ADC and
before the next kernel-related operation, closing a freeze that otherwise depended on the board's specific ADC
clock divider to show up at all.

**Fix:** after disabling the G0/C0/WL ADC, wait at least 2 ADC kernel clock periods before re-enabling it,
starting calibration, or writing the calibration factor register — compute the wait from the actual configured
kernel clock, don't hardcode a microsecond figure that assumes one clock setting.

**Detection:** check: src mid · chip: G0 — needs the ADC kernel clock configuration to compute whether an
observed delay between disable and the next operation is actually long enough.

## <a id="err-9"></a>`ERR-9` — G0 I2C: kernel clock fast enough for the transmitter's setup time, and never write TXDR right at the deadline

**ES0548 2.11.1** ("Wrong data sampling when data setup time (tSU;DAT) is shorter than one I2C kernel clock
period"):

> The I2C-bus specification and user manual specify a minimum data setup time (tSU;DAT) as: 250 ns in Standard
> mode, 100 ns in Fast mode, 50 ns in Fast mode Plus. The device does not correctly sample the I2C-bus SDA line
> when tSU;DAT is smaller than one I2C kernel clock (I2C-bus peripheral clock) period: the previous SDA value is
> sampled instead of the current one. This can result in a wrong receipt of target address, data byte, or
> acknowledge bit.

Workaround: increase the I2C kernel clock so its period fits inside the transmitter's setup time — **at least
4 MHz in Standard mode, 10 MHz in Fast mode, 20 MHz in Fast-mode Plus**, for a transmitter at the I2C-bus
standard's own minimum setup times.

**ES0548 2.11.4** ("Transmission stalled after first byte transfer"):

> When the first byte to transmit is not prepared in the TXDATA register, two bytes are required successively,
> through TXIS status flag setting or through a DMA request. If the first of the two bytes is written in the
> I2C_TXDR register in less than two I2C kernel clock cycles after the TXIS/DMA request, and the ratio between
> APB clock and I2C kernel clock frequencies is between 1.5 and 3, the second byte written in the I2C_TXDR is not
> internally detected. This causes a state in which the I2C peripheral is stalled...

Workaround: write the first byte to `I2C_TXDR` *before* the transmission starts, **or** keep the
APB-clock-to-I2C-kernel-clock ratio outside 1.5-3.

**Example:** donguru_template's I2C bring-up checks the kernel-to-APB clock ratio for exactly this reason before
accepting a clock configuration, rather than discovering a stall empirically on one particular clock setting.

**Fix:** when configuring G0 I2C, check the achieved I2C kernel clock against both errata at once: fast enough
that its period fits the bus mode's minimum setup time (2.11.1), and its ratio to APB clock outside 1.5-3, or the
first TXDR write made before the transmission starts (2.11.4).

**Detection:** check: review — both errata depend on the achieved kernel clock vs. APB ratio, which needs the
clock-tree configuration cross-checked against the sheet's numeric ranges, not a source pattern.

## <a id="err-10"></a>`ERR-10` — Cache errata before trusting a hang is somewhere else

Turning on an instruction or data cache (D-cache on Cortex-M7 parts in particular, per `DMA-2`) changes timing
and coherency behavior in ways a part's own errata sheet sometimes documents as outright bugs — a cache that
can serve stale data after a specific sequence, or that interacts badly with a specific bus master. A hang or
data-corruption bug that appears only with cache on is exactly the shape of bug the errata sheet is most likely
to already describe.

**Fix:** before debugging a cache-adjacent hang by other means, check the part's errata sheet's cache-related
entries for the silicon revision in use; a documented erratum with a known workaround is cheaper to apply than
to rediscover.

**Detection:** check: review — this is a diagnostic habit (check the errata before bisecting), not a pattern a
lint can enforce; see also `TEST-6`.

## <a id="err-11"></a>`ERR-11` — RP2350's own revision register can't tell every stepping apart

The RP2350 datasheet's hardware-revision-history appendix gives the fact directly:

> Stepping A3 is identified by a CHIP_ID.REVISION value of 0x3, and a bootrom version byte at offset 0x00000013
> of 0x3... Stepping A4 is identified by a CHIP_ID.REVISION value of 0x3, and a bootrom version byte at offset
> 0x00000013 of 0x4.

`CHIP_ID.REVISION` alone reads **0x3 for both A3 and A4** — reading only that register and assuming it uniquely
identifies the stepping silently conflates two revisions with different fixed errata (A3 fixes several bootrom
issues that A4 doesn't add more fixes for, but a caller that only checks `CHIP_ID.REVISION == 0x3` can't tell
which fixes are actually present).

**Example:** this repo's chip-variant work (`TPL-55`) reads the bootrom version byte at offset `0x00000013`
instead of (or in addition to) `CHIP_ID.REVISION`, specifically because that byte is what actually distinguishes
A3 (bootrom 3) from A4 (bootrom 4); RP2040 has the same shape of problem one level further back (B0/B1/B2 map to
bootrom versions 1/2/3, per the RP2040 datasheet).

**Fix:** identify the RP2040/RP2350 silicon stepping from the bootrom version byte, not from `CHIP_ID.REVISION`
alone, whenever the firmware needs to know which errata workarounds still apply on the board it's running on.

**Detection:** check: src easy · chip: RP2350 — a chip-identification path that reads `CHIP_ID.REVISION` and
never reads the bootrom version byte is a straightforward thing to flag.
