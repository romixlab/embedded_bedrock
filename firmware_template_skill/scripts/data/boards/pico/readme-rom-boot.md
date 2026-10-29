**BOOTSEL.** `{cli} boot reboot --updater` resets the board into the RP2040 boot ROM, and so does the firmware
itself after four boots in a row that each ended within 10 s (a boot loop: the next firmware reports it as
`last result  Recovery` in `{cli} dfu`). The boot ROM has no USB DFU like an STM32: it mounts a `RPI-RP2` USB mass
storage drive and takes a `.uf2` dropped on it over PICOBOOT, which `{cli} dfu rp flash` (or `just flash`, through
the Debug Probe instead) drives for you. Holding BOOTSEL while plugging the USB cable in reaches the same boot ROM
by hand, with no firmware involved at all. The boot ROM stays until the next reset (unplug and plug back in, or
BOOTSEL + RUN on a board with the RUN pin wired out), which starts the firmware again.
