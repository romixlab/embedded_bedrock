**The ROM bootloader.** `{cli} boot reboot --updater` resets the board into the STM32 ROM bootloader, and so does
the firmware itself after four boots in a row that each ended within 10 s (a boot loop: the next firmware reports
it as `last result  Recovery` in `{cli} dfu`). The ROM bootloader's USB DFU is on PA11 / PA12, port 1 of the hub,
which keeps running through the reset (PC13 on RTC_OUT1): the PC sees an `STM32  BOOTLOADER` DFU device behind the
Donguru's hub on the same cable. The button SW102 held while the board powers up starts the ROM bootloader too
(if the option bytes let the BOOT0 pin decide), but on rev A without USB: a power-up clears RTC_CR, R304 holds the
hub in reset and no firmware runs to release it. ST's AN2606 lists the ROM's other interfaces. The ROM bootloader stays until the
next reset (unplug and plug in again), which starts the firmware again; the probe reaches the chip whatever runs
on it, so `just flash` works from the ROM bootloader too. Flashing four times within 10 s of each other counts as
a boot loop as well: the fourth boot lands in the ROM bootloader, the next reset or flash comes back.
