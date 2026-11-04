# FLS checks, explained

Detail page for the `FLS` category of the [check catalogue](../checks.md#-fls---flash-and-ecc). Each section
explains one check: what it catches, why the firmware fails without it, a concrete example, how it is checked, and
the fix. The one-line entries in the catalogue stay the source of truth for ID, severity and wording; this page is
the commentary.

## `FLS-1` Torn flash read raw on an ECC part <a id="fls-1"></a>

🛑 error · check: src mid · chip: G0, H7, G4, L4... · [catalogue entry](../checks.md#fls-1)

**What it catches.** Flash that a power cut can leave torn (settings, bootloader state) read directly instead of
through `ww_ecc_flash`'s NMI-hook-protected reader, on a part whose flash has ECC.

**Why it matters.** ECC flash computes an ECC code per word at write time; a word that was only partially written
when power cut has a data/ECC mismatch, and reading it raw triggers a double-ECC-error trap — an NMI on G0, a
BusFault on H7 — right at the read, not at the write that caused it. A reader that doesn't expect this treats a
perfectly ordinary power-cut artifact as a fatal fault.

**Example.** typec_dongle API-19, and the settings design in the template both exist because of this; upstream
embassy issue #7235 covers the same hazard from the embassy-boot side.

**How to check.** `src mid`, limited to ECC-flash parts (G0, H7, G4, L4 and others per the chip table): a read of
settings/bootloader-state flash regions that doesn't go through `ww_ecc_flash`'s NMI-hooked accessor.

**Fix.** Read torn-writable flash regions only through `ww_ecc_flash` (or an equivalent NMI/BusFault-hooked
reader) so a double-ECC error from a half-written word is handled as "this word was never fully written", not as a
fault.

## `FLS-2` More than one owner of the flash peripheral <a id="fls-2"></a>

🛑 error · check: src mid · [catalogue entry](../checks.md#fls-2)

**What it catches.** Two independent pieces of code — a firmware updater and a settings store, most often —
each taking their own `FLASH` peripheral handle instead of sharing one owner.

**Why it matters.** Flash programming on most of these parts is a shared, stateful peripheral (one set of
control registers, one busy flag); two independent owners racing an erase and a write against each other corrupts
whichever one loses, and because both "succeed" from their own point of view, the corruption surfaces later, far
from the code that caused it.

**Example.** embedded_bedrock TPL-24: the updater and the settings store both wanting `FLASH` directly is exactly
the shape this check exists for.

**How to check.** `src mid`: the firmware model should show exactly one place that owns the `FLASH` peripheral
singleton; any second independent acquisition is the finding.

**Fix.** Give flash access to one owner (a flash-manager task, or the updater and settings sharing a single handle
behind a mutex) and route every consumer through it.

## `FLS-3` RP flash operations unsafe against core 1 or DMA XIP reads <a id="fls-3"></a>

🛑 error · check: src mid · chip: RP · [catalogue entry](../checks.md#fls-3)

**What it catches.** RP2040/RP2350 flash erase or write issued from core 1, or with core 1 running outside
`spawn_core1`'s controlled start-up, or while DMA is reading flash through XIP.

**Why it matters.** RP's flash program/erase routines execute from RAM precisely because flash can't be read
(including XIP-mapped code fetches) while it's being erased or written; core 1 running arbitrary code during a
flash op can stall on an XIP read exactly when the flash isn't available to answer it, and DMA reading flash
during the op hits the same stall from a different direction.

**Example.** embedded_bedrock TPL-25 is this exact hazard worked through for the RP flash path the template
generates.

**How to check.** `src mid`, chip RP: flash erase/write call sites, checked against whether core 1 is confirmed
parked (via `spawn_core1`'s synchronisation) and whether any DMA channel is configured to read flash concurrently.

**Fix.** Park core 1 (or route its own flash needs through the same serialised owner, `FLS-2`) and make sure no
DMA channel is mid-transfer from flash before issuing an erase or write.

## `FLS-4` No periodic image CRC or ECC left unused <a id="fls-4"></a>

⚠️ warning · check: src easy · [catalogue entry](../checks.md#fls-4)

**What it catches.** No periodic check of the running image's CRC in flash, and no use of the part's own ECC on
flash/RAM where the chip actually has it.

**Why it matters.** Flash bits can flip over the firmware's lifetime (radiation, wear, marginal cells); without a
periodic CRC of the image itself, or without ECC turned on where available, the first sign of a flipped bit is
whatever misbehaviour the corrupted instruction or data happens to cause — not a clean, early detection.

**How to check.** `src easy`: a periodic task computing a CRC over the running image's flash range, and ECC
enabled on parts whose chip data lists it.

**Fix.** Add a low-priority periodic CRC check over the application image; turn on the chip's flash/RAM ECC
where available rather than leaving it off by default.

## `FLS-5` Slow erase without checking programming parallelism <a id="fls-5"></a>

💡 suggestion · check: review · [catalogue entry](../checks.md#fls-5)

**What it catches.** A flash erase that takes longer than expected, investigated without first checking the
part's programming-parallelism setting (which trades peak current for speed).

**Why it matters.** Programming parallelism is a direct, documented knob on erase/write speed and peak current;
chasing a slow-erase symptom through other theories before checking this setting wastes time on a question the
datasheet already answers.

**How to check.** Not automated (`review`): a reminder for whoever is debugging erase timing, not a pattern to
match.

**Fix.** Check the configured programming parallelism against the part's datasheet table before looking elsewhere;
raising it trades more peak current for a faster erase/write.

## `FLS-6` Power-fail flash behaviour not tested on the family in use <a id="fls-6"></a>

⚠️ warning · check: test · [catalogue entry](../checks.md#fls-6)

**What it catches.** A settings or bootloader-state store whose power-cut behaviour during erase and write was
never tested against the specific flash family's own quirks — L0/L1 erasing to `0x00` instead of `0xFF`, U5
refusing to program zeros over zeros, H7 flash words that can only be programmed once.

**Why it matters.** `FLS-1`'s torn-read protection and `BOOT-5`'s ordering logic both assume a particular model of
"what does a torn write look like"; that model is different per flash family, and a power-fail test written
against one family's behaviour can pass cleanly while hiding a real bug on another.

**Example.** chunky_mcu_updater's fuzz harness, and an internal log entry (Oct 2026) specifically about these per-family
differences being easy to miss when porting a settings store between chip families.

**How to check.** `test`: a power-cut fuzz harness parameterised per flash family's documented erase/program
quirks, run against the actual family the firmware targets.

**Fix.** Write (or select) the power-fail test for the specific flash family in use, not a generic one copied from
a different chip's test suite.

## `FLS-7` Unused program memory not filled with a trap <a id="fls-7"></a>

⚠️ warning · check: elf easy · [catalogue entry](../checks.md#fls-7)

**What it catches.** Flash space past the linked image, before the next used region, left at its erased value
(typically `0xFF`) instead of filled with an illegal-opcode trap or write-protected.

**Why it matters.** Erased flash executed by mistake (a corrupted jump, a runaway PC per `FLT-12`'s AN1015 class
of upset) decodes as a long run of NOPs or undefined behaviour on most ISAs, letting execution wander further
before anything notices; a trap opcode faults immediately at the first byte reached.

**Example.** The Notion firmware checklist lists this as a standard hardening step most firmwares skip simply
because nothing in the default toolchain does it automatically.

**How to check.** `elf easy`: unused flash ranges in the linked ELF should contain the chosen trap pattern, not
the erased value.

**Fix.** Add a post-link step (or a linker fill directive) that writes an illegal-opcode trap pattern across
unused flash instead of leaving it erased.

## See also

- [Review checklist](../review-checklist.md) — *Memory and flash* condenses FLS-1 and FLS-2.
- [Bring-up checklist](../bringup-checklist.md) step 6 is `FLS-6` alongside `BOOT-5` and `PWR-5`.
- [`MEM-8`](mem.md#mem-8) — the settings-region sizing and bank placement that `FLS-1`'s torn-read reader
  protects.
- [`FLT-12`](flt.md#flt-12) — the AN1015 runaway-execution class of upset `FLS-7`'s trap fill helps catch.
