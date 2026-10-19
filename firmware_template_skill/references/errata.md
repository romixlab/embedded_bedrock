# Chip errata

Firmware that relies on a chip's peripherals, clocks, flash, DMA, low-power modes or boot path reads the chip's
errata sheet first, for the silicon revision on the board, and quotes errata from the document itself (section,
title, status per revision, the vendor's workaround), not from memory or a forum summary. `bedrock_gen.py new`
prints where the sheet is (`note: errata: ...`, also in the generated `README.md`).

## Where the errata are

| vendor | document | revision on the board |
|---|---|---|
| STM32 | device errata sheet `ESxxxx` ("STM32G0B1xB/xC/xE device errata"), linked from the part's st.com page; `bedrock_gen.py chip-info --chip <part>` lists it (`doc: errata_sheet:`). Also Jan Waclawek's [STM32 gotchas](http://www.efton.sk/STM32/gotcha/), which collects reference-manual traps the errata don't list. | `DBGMCU_IDCODE.REV_ID`, mapped to the marking in the sheet's "Device variants" table |
| RP2040 / RP235x | the "Errata" appendix of the RP2040 / RP2350 datasheet (`RP2040-E1`...) | `CHIP_ID` / `SYSINFO`, the marking (B2, A2, A4, ...) |
| nRF | Nordic's errata document for the IC revision (docs.nordicsemi.com) | `FICR.INFO.VARIANT`, the marking |
| ESP | Espressif's "Series SoC Errata" for the chip | the chip revision `esptool` prints |

## Errata the template handles

- **ES0548 2.2.10 "Prefetch failure when branching across flash memory banks"** (STM32G0B1, also check G0C1's sheet):
  status N (not fixed) on revisions A, Z/3 and Y/4; "In rare cases, the code prefetch may fail upon branching and
  function calls across flash memory banks, when the device operates in dual-bank mode. The failing prefetch then
  provides incorrect data to the CPU, which corrupts code execution and may lead to HardFault interrupt."
  Applies to 512 KiB parts (always dual-bank) and 256 KiB parts with `DUAL_BANK` set; workaround "None" (prefetch
  off, or no code branching across the banks). Seen on Donguru: a call from bank 2 into bank 1 HardFaulted under USB
  load. The template turns flash prefetch on for G0 with `--framework embassy` (embassy main does it itself) and, on
  a dual-bank G0B1 / G0C1, adds `ASSERT(__etext <= <bank 2 base>)` to `memory.x`, so code reaching bank 2 fails to
  link instead of faulting at run time; the fix then is prefetch off in `src/main.rs` (and dropping the ASSERT) or a
  smaller image (`opt-level = "s"`).
