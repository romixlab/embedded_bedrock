# bedrock_board_cli

`boardmap`: board infographics (top and bottom views, power tree, pin table) from a PCB file and a TOML description (`bedrock_board`).

```sh
cargo install --git https://github.com/vhrdtech/embedded_bedrock bedrock_board_cli
boardmap --help
source <(COMPLETE=bash boardmap)
```

The board description `board.toml` is checked by spirit_level (`spl check`, the `brd-*` lints); `bedrock-board gen` (BOARD-4) will join `boardmap` here.
