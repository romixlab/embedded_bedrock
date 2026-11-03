# Firmware bring-up checklist

What to try on hardware when firmware first runs on a board, after a board revision, and before a release. Condensed
from the [check catalogue](checks.md) (the `bench` and `test` entries); each bracketed ID links to its entry. The
board side (rails, crystals, DNP parts) is the board bring-up checklist in tpm (`roles/checklists/bringup.md`).

1. *Silicon:* read the silicon revision fitted and the errata sheet for it [[ERR-1]](checks.md#err-1),
   [[ERR-11]](checks.md#err-11). Check the HAL's chip feature is the part on the board [[CFG-5]](checks.md#cfg-5).
2. *Safe state:* scope the outputs from power-on through the first second: nothing enabled before the firmware's
   safe state, translators enabled last [[PIN-2]](checks.md#pin-2), [[PIN-3]](checks.md#pin-3). Read every input
   with the board idle: pulls fitted as the BOM says [[PIN-5]](checks.md#pin-5).
3. *Clocks:* measure the core clock once with DWT against the time driver [[CLK-1]](checks.md#clk-1). Check the
   LSE start-up time [[CLK-4]](checks.md#clk-4).
4. *Stack and faults:* provoke a stack overflow, a HardFault and a panic; each ends in a reset that the next boot
   reports with the ELF symbolized [[STK-7]](checks.md#stk-7), [[FLT-10]](checks.md#flt-10). Check the reset cause
   for power-on, pin, watchdog and software reset [[FLT-5]](checks.md#flt-5), and a crash right after a cold
   power-up [[FLT-14]](checks.md#flt-14).
5. *Boot and update:* enter the ROM bootloader without a probe and come back; update from the previous released
   version over the field path [[BOOT-4]](checks.md#boot-4), [[BOOT-1]](checks.md#boot-1),
   [[BOOT-7]](checks.md#boot-7). On H7, after a ROM DFU session, check the supply config the firmware runs on
   [[BOOT-2]](checks.md#boot-2). Make a boot loop and see the rescue [[FLT-8]](checks.md#flt-8).
6. *Power cuts:* cut the supply during a settings write and a bootloader state change; measure how long the MCU
   runs after the cut [[FLS-6]](checks.md#fls-6), [[BOOT-5]](checks.md#boot-5), [[PWR-5]](checks.md#pwr-5).
7. *Load:* run the busiest host traffic with logging on and the probe attached: no overruns, no retries, counters at
   zero [[EXE-1]](checks.md#exe-1), [[DMA-4]](checks.md#dma-4), [[LOG-1]](checks.md#log-1). Kill the host without
   a disconnect [[USB-4]](checks.md#usb-4).
8. *External chips:* dip each shared rail and check the chips come back configured [[EXT-2]](checks.md#ext-2); one
   probe owner per board while measuring [[TEST-5]](checks.md#test-5).
9. *Results:* every finding goes into the firmware's FEATURES.md, and a lesson worth keeping becomes a catalogue
   entry with its source ([checks.md](checks.md) "Sources").
