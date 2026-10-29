`just pins` is `ww pins tui` with `docs/board/b135b.toml` as its board preset: J203 drawn as on the board, each pad
with its level (`●` high, `○` low) and what was set on it. `ww pins test` drives and pulls each pin and reads it
back: a pin shorted to GND, to 3V3 or to another pin, or one with an external pull, shows up;
`docs/board/b135b-pins-test.toml` holds what a bare board shows.

**Jumper test:** put a jumper between J203-5 and J203-7 (PE0 and PB4). In `just pins`, drive PE0 high (`1`) and
low (`0`): PB4 follows. `ww --api {api} pins test PE0,PB4` reports them shorted to each other; take the jumper off
and they are not.
