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

- **ES0491 (STM32H72x/73x, Rev 9) for USB_OTG_HS** (`--usb-phy ulpi`, TPL-54): its only OTG_HS item is 2.23.1 "Host
  packet transmission may hang when connecting the full speed interface through a hub to a low-speed device",
  status N on revisions Z and A, workaround "None. However, increasing the capacitance on the data lines may reduce
  the occurrence." It is about host mode on the DP/DM full-speed interface; the template's device mode, FS or ULPI,
  is not affected and needs nothing. The PC2 / PC3 ULPI pins (DIR, NXT) are `PC2_C` / `PC3_C` on LQFP176 packages:
  embassy-stm32 leaves the analog switches closed unless its `split-pc2` / `split-pc3` features are on, so they work.
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

- **FDCAN, `--can`** (`src/can.rs`): ES0548 Rev 4 2.15 (STM32G0B1/G0C1) and ES0491 Rev 9 2.22 (STM32H72x/H73x), status
  A (workaround available) on every revision the sheets list:
  - ES0548 2.15.1 / ES0491 2.22.1 "Desynchronization under specific condition with edge filtering enabled": "FDCAN
    may desynchronize and incorrectly receive the first bit of the frame if the edge filtering is enabled (the EFBI
    bit of the FDCAN_CCCR register is set), and the end of the integration phase coincides with a falling edge
    detected on the FDCAN_Rx input pin"; workaround "Disable edge filtering or wait for frame retransmission". Edge
    filtering stays off unless the host configures it.
  - ES0548 2.15.2 / ES0491 2.22.2 "Tx FIFO messages inverted under specific buffer usage and priority setting": only
    when "FDCAN uses both a dedicated Tx buffer and a Tx FIFO (the TFQM bit of the FDCAN_TXBC register is cleared)".
    The firmware uses the Tx FIFO alone, which is the sheets' second workaround ("Use only a Tx FIFO").
  - ES0491 2.22.3 "DAR mode transmission failure due to lost arbitration": "In DAR mode, the transmission may fail
    due to lost arbitration at the first two identifier bits"; workaround "clear the corresponding Tx buffer
    transmission request bit TRPx of the FDCAN_TXBRP register and set the corresponding cancellation finished bit
    CFx of the FDCAN_TXBCF register, then restart the transmission". **Not handled**: DAR is the API's `one_shot`
    option, a frame lost this way is dropped (comment in `src/can.rs`, FEATURES TPL-53).
