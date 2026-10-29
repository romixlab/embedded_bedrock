`just pins` is `ww pins tui` with `docs/board/report_b200a.toml` as its board preset. No MCU pin reaches a header
directly on this board: J102 sits behind translators that the 595 chain keeps off, so the pins served are the MCU
sides of those translators, the microSD card's lines and the sync line on J103. `ww pins test` drives and pulls each
pin and reads it back: a pin shorted to GND, to 3V3 or to another pin, or one with an external pull, shows up;
`docs/board/report_b200a-pins-test.toml` holds what a bare board shows (the SD card's and I2C's pull-ups, the
direction pins' pull-downs, GP26 and GP34 on one net).

**Jumper test:** put a jumper on J103 pins 1-2 (SYNC on GP0, and GND). In `just pins`, pull GP0 up: it stays low.
`ww --api {api} pins test GP0` reports it shorted to GND; take the jumper off and it floats again. SW101 is not a
pin here: it is `buttons.sw101` in the API, `true` while pressed.
