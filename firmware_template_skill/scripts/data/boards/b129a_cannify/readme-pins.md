`just pins` is `ww pins tui` with `docs/board/b129a_cannify.toml` as its board preset: J1 and J2 drawn as on the
board, each pad with its level (`●` high, `○` low) and what was set on it. `ww pins test` drives and pulls each pin
and reads it back: a pin shorted to GND, to 3V3 or to another pin, or one with an external pull, shows up;
`docs/board/b129a_cannify-pins-test.toml` holds what a bare board shows.

**Jumper test:** put a jumper on J1 pins 1-2 (PB4 and PB5). In `just pins`, drive PB4 high (`1`) and low (`0`): PB5
follows. `ww --api {api} pins test PB4,PB5` reports them shorted to each other; take the jumper off and they
are not.
