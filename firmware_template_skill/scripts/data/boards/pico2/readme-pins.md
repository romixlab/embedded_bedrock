`just pins` is `ww pins tui` with `docs/board/pico2.toml` as its board preset: J0 drawn as on the board (left column
pins 1-20, right column 21-40) plus J2 debug, each pad with its level (`●` high, `○` low) and what was set on it.
`ww pins test` drives and pulls each pin and reads it back: a pin shorted to GND, to 3V3 or to another pin, or one
with an external pull, shows up; `docs/board/pico2-pins-test.toml` holds what a bare board shows.
