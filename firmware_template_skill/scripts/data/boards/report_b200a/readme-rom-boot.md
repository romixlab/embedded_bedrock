**BOOTSEL by hand.** The board has no BOOT button: short J103 pin 5 (`BOOT_EXT`, to QSPI_SS through 255 Ω and 1K) to
pin 6 (GND) while plugging the USB-C cable in, and the RP2350 starts its boot ROM instead of the firmware: the PC
sees an `RP2350` USB mass storage drive and the PICOBOOT interface. Pin 5 is for shorting to GND only, never for a
voltage. `{cli} boot reboot --updater` gets there without the jumper while the firmware runs.
