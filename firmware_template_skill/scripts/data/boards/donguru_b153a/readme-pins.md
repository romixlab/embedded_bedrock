`just pins` is `ww pins tui` with `docs/board/donguru_b153a.toml` as its board preset: J101 drawn as on the board,
each pad with its level (`●` high, `○` low) and what was set on it. `ww pins test` drives and pulls each pin and
reads it back: a pin shorted to GND, to 3V3 or to another pin, or one with an external pull, shows up;
`docs/board/donguru_b153a-pins-test.toml` holds what a bare board shows. The net ties show up as shorts on purpose:
PB1 with PB3 (J101-1), PB7 with PB8 (SBU1_VT), PB6, PB9 and PB11 (SBU2_VT).

**Jumper test:** put a jumper between J101-2 (PB4) and J101-3 (PA2). In `just pins`, drive PB4 high (`1`) and low
(`0`): PA2 follows (the two 255 Ω in series don't matter for a GPIO). `ww --api {api} pins test PB4,PA2` reports
them shorted to each other; take the jumper off and they are not.
