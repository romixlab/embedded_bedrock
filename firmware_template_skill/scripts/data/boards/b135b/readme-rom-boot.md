**The ROM bootloader.** `{cli} boot reboot --updater` resets the board into the STM32 ROM bootloader, and so do
holding SW201 (BOOT0) while the board resets and the firmware itself after four boots in a row that each ended
within 10 s (a boot loop: the next firmware reports it as `last result  Recovery` in `{cli} dfu`). The ROM
bootloader's USB DFU is on PA11 / PA12, which the TS3USB221A U301 connects to J301 at reset (R305 pulls its select
high), so the PC sees an `STM32  BOOTLOADER` DFU device, full speed, on the same cable. ST's AN2606 lists the ROM's
other interfaces. The ROM bootloader stays until the next reset (J202-10, a power cycle), which starts the firmware
again; the probe reaches the chip whatever runs on it, so `just flash` works from the ROM bootloader too. Flashing
four times within 10 s of each other counts as a boot loop as well: the fourth boot lands in the ROM bootloader,
the next reset or flash comes back.

**After the ROM bootloader, the supply.** ROM bootloader versions before V9.3 (AN2606) lock PWR_CR3, the supply
configuration, in its reset state (SMPS and LDO) when they hand over; PWR_CR3 takes one write per power-on, so the
firmware's `SupplyConfig::DirectSMPS` that follows is ignored until the next power cycle. On this board that still
works: VDDLDO, VCAP and VFBSMPS are one net. The firmware does not report it yet (FEATURES.md TPL-69);
vb135_fdcan_iso_usb_fw's `supply_config()` reads PWR_CR3 back and counts it. ES0491 2.2.5: the ROM's USB DFU does
not work with readout protection level 1 set.
