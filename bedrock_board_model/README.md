# bedrock_board_model

The board description: one typed `board.toml` per board that says which MCU pin does what (GPIOs with their level from reset, peripherals with their signals, LEDs, buttons, reserved and shorted pins), for every revision and rework of the board.

- `Description::load` reads the file with source spans; mistakes come back as diagnostics pointing at the line.
- Revisions (`[rev.A]`) and reworks (`[rework.5]`) are patches over the base; `configurations()` resolves each one into a complete board.
