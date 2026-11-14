# bedrock_board_model

The board description: one typed `board.toml` per board that says which MCU pin does what (GPIOs with their level from reset, peripherals with their signals, LEDs, buttons, reserved and shorted pins), for every revision and rework of the board.

- `Description::load` reads the file with source spans; mistakes come back as diagnostics pointing at the line.
- Revisions (`[rev.A]`) and reworks (`[rework.5]`) are patches over the base; `configurations()` resolves each one into a complete board.

This crate is the schema's one loader. The checks against the chip's data and the PCB's nets are spirit_level's board lints: `spl check` in the firmware repo, embedded tier (`brd-1` to `brd-7`). The format: [book/board-description.md](../book/board-description.md). B135's description, loaded in the tests, is in `tests/boards/b135/`.
