`reboot(Updater)` resets the RP2350 straight into the boot ROM's BOOTSEL mode (no button needed once the firmware is
running): it re-appears as a PICOBOOT and USB mass storage device, `ww dfu rp flash` (or `just flash`) writes the new
firmware and it starts running on its own. Holding the BOOTSEL button while plugging the USB in does the same from a
cold, unflashed board.
