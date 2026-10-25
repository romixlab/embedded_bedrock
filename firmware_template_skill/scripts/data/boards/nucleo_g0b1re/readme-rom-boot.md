**The ROM bootloader.** `{cli} boot reboot --updater` resets the board into the STM32 ROM bootloader, and so does
the firmware itself after four boots in a row that each ended within 10 s (a boot loop: the next firmware reports
it as `last result  Recovery` in `{cli} dfu`). The ROM bootloader's USB DFU is on PA11 / PA12 (USB D- on CN10-14, D+ on CN10-12), which
a Nucleo-64 brings only to the morpho header CN10, not to a USB connector: to use it, wire a USB cable to those pins
and GND. ST's AN2606 lists the ROM's other interfaces. The ROM bootloader stays until the next reset (B2, a power
cycle), which starts the firmware again; the probe reaches the chip whatever runs on it, so `just flash` works from
the ROM bootloader too. Flashing four times within 10 s of each other counts as a boot loop as well: the fourth boot
lands in the ROM bootloader, the next reset or flash comes back.
