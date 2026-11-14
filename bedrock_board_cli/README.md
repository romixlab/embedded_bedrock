# bedrock_board_cli

Two binaries over the board crates:

- `boardmap`: board infographics (top and bottom views, power tree, pin table) from a PCB file and a TOML description (`bedrock_board`).
- `bedrock-board`: the board description `board.toml` (`bedrock_board_model`). `bedrock-board check [board.toml]` holds it against the chip's data and the PCB's nets for every revision and rework; `--format json` for tools.

```sh
cargo install --git https://github.com/vhrdtech/embedded_bedrock bedrock_board_cli
bedrock-board check boards/b135/board.toml
source <(COMPLETE=bash bedrock-board)
```
