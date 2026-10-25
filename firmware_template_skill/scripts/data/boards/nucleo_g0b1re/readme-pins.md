`just pins` is `ww pins tui` with `docs/board/nucleo_g0b1re.toml` as its board preset: CN7, CN10 and the Arduino
headers drawn as on the board, each pad with its level (`●` high, `○` low) and what was set on it. `ww pins test`
drives and pulls each pin and reads it back: a pin shorted to GND, to 3V3 or to another pin, or one with an external
pull, shows up; `docs/board/nucleo_g0b1re-pins-test.toml` holds what a bare board shows.

**Jumper test:** put a jumper on CN7 pins 1-2 (PC10 and PC11). In `just pins`, drive PC10 high (`1`) and low (`0`):
PC11 follows. `ww --rtt {chip} pins test PC10,PC11` reports them shorted to each other; take the jumper off
and they are not. Press B1 and PC13 reads low.
