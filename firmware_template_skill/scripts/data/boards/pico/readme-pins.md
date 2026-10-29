`just pins` is `ww pins tui` with `docs/board/pico.toml` as its board preset: the two edges drawn as on the board,
each pad with its level (`●` high, `○` low) and what was set on it. `ww pins test` drives and pulls each pin and
reads it back: a pin shorted to GND, to 3V3 or to another pin, or one with an external pull, shows up;
`docs/board/pico-pins-test.toml` holds what a bare board shows.

**A quick check without any wiring:** in `just pins`, GP24 (VBUS sense, read-only) reads high whenever the board
runs from USB power, which it is doing right now.
