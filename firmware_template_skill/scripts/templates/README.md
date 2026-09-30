# bedrock_gen templates

Rendered with Jinja2 (`trim_blocks`, `lstrip_blocks`, `keep_trailing_newline`, `StrictUndefined`):

* A line holding only a `{% ... %}` tag disappears completely (tag, indentation and newline), so block
  tags can be indented to match the surrounding code.
* A blank line that only exists when an optional block is present goes *inside* that block, as its first
  line.
* A `{% ... %}` tag at the very end of a text line also eats that line's newline; use `{{ "x" if c else "" }}`
  for inline conditionals instead.
* `_name.j2` files are partials, pulled in with `{% include %}`.

Layout:

| path | output |
|---|---|
| `app/` | application crate; `build.rs`, `cargo_config.toml`, `rust-toolchain.toml`, `memory.x` are shared with the bootloader; `AGENTS.md.j2`/`CLAUDE.md` describe the template upgrade flow (`bedrock_fw.json` itself is written by `fw_json()` in the script) |
| `app/src/main/` | one `main.rs` body per framework/family, included by `app/src/main.rs.j2` |
| `bootloader/` | embassy-boot bootloader crate |
| `ww/` | `--wire-weaver`: API crate (`ww/api/` → `<ww-api>/`) and the project `README.md`; the firmware side is `app/src/ww.rs.j2` + `app/src/main/_rtt_init.rs.j2` |
| `linker/` | static linker script fragments |
| `hubris/` | `hubris-memory` output |

With `--wire-weaver` every app/bootloader file is moved under `firmware/`; `AGENTS.md`/`CLAUDE.md` and the `ww/` files
go to the output directory. `root` ("../" or "") leads from the firmware crate to the output directory, which
`--bedrock`/`--ww-src` paths are relative to; `ww` holds the API names and transport details (`ww_context()`).

Context available in every template: `t` (Target), `o` (Opts), `V` (crate versions), `BEDROCK_GIT`, `WW_GIT`, filters
`toml_list`, `hex`, `hex8`, `fmt_len`, function `feats(...)` (drops falsy entries). See `render()` in
`../bedrock_gen.py` for per-template extras.
