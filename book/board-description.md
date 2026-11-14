# Board description (`board.toml`)

One typed file per board says which MCU pin does what, for every revision and rework of the board. `bedrock-board check` holds it against the chip's data and the PCB's nets. The library is `bedrock_board_model`.

The file is written per board, beside the firmware that runs on it. The PCB stays the truth for copper: the description names pins, and optionally the net a pin must be on, and never repeats the connectivity.

## The file

```toml
[board]
name = "B135"
chip = "STM32H725IG"          # as stm32-data names it
package = "UFBGA176"          # optional: pins are checked against this package
mcu = "U201"                  # the MCU's reference on the PCB
pcb = "b135b.geometry.json"   # a .kicad_pcb or boardmap's geometry JSON, relative to this file
revisions = ["A", "B"]        # every revision one firmware supports; leave out for a board with one

[gpio.ulpi_rst]
pin = { pin = "PH3", net = "~{ULPI_RST}" }   # or pin = "PH3"
mode = "output"               # output (needs init), input (needs pull), analog
init = "high"                 # the level from reset on

[periph.can1]
instance = "FDCAN1"
pins = { TX = "PB9", RX = { pin = "PB8", net = "FDCAN1_RX" } }   # signals as stm32-data names them

[led.status]
rgb = ["PC7", "PC6", "PH13"]  # or pin = "PA5" for one colour
active = "low"                # the level that lights it

[button.boot0]
pin = "PE1"
active = "high"               # the level while pressed
pull = "none"

[reserved.swd]
pins = ["PA13", "PA14"]       # left in their reset function

[short.j203_3]
pins = ["PG9", "PB3"]         # joined on the board: at most one is used, the others stay inputs

[free]
PE0 = "J203-5"                # a pin with a net and no job on the board

[rev.A]                       # what differs on rev A
pcb = "b135a.geometry.json"
note = "two RGB LEDs"
remove = ["led.status"]       # SECTION.NAME

[rev.A.led.can]               # added, or replacing the base entry of that name whole
rgb = ["PC7", "PC6", "PA10"]
active = "low"

[rework.5]
revisions = ["A"]             # the revisions it applies to; leave out for all
note = "SD lines moved"
```

Every entry may have a `note`. Unknown keys are refused.

## Configurations

The checks run on every configuration: each revision, each revision with each rework that applies to it, and each revision with all of them. Two reworks that change the same entry are refused (`rework-overlap`), since fitted together the result would be ambiguous. A finding names the configurations it holds in when that is not all of them.

## Checks

| Code | Finds |
|---|---|
| `pin-twice` | a pin used by two entries |
| `free-used` | a `[free]` pin some entry uses |
| `short-both` | two pins of one `[short]` both used (a free one counts) |
| `pin-unknown`, `package-unknown` | a pin the chip or its package does not have |
| `periph-unknown`, `signal-unknown` | a peripheral or a signal the chip does not have |
| `signal-pin` | a signal on a pin that cannot carry it; help names the pins that can, with their AF |
| `pcb-unconnected`, `pcb-pad` | a used pin not connected on the PCB, or not on the MCU footprint |
| `pcb-net` | a pin on another net than its `net` says (the whole name or its last `/` segment) |
| `pcb-shared`, `pcb-short` | MCU pins on one net without a `[short]`; a `[short]` across different nets |
| `pcb-undescribed` | a connected pin the description does not name (warning) |

Structural mistakes (an output without `init`, `rgb` without three pins, a `[rev.X]` for no listed revision, removing what is not there) are reported when the file loads, each at its line.

## Chip data

STM32 pin and signal data comes from stm32-data's chip JSON (`<CHIP>.json` of embassy-rs/stm32-data-generated `data/chips`), the source of stm32-metapac's metadata. stm32-metapac itself holds one chip per build, so the host tool reads the JSON. `bedrock-board check` looks in `--chip-data DIR`, `BEDROCK_CHIP_DATA`, or the firmware template generator's cache `~/.cache/bedrock_gen/chips` (`bedrock_gen.py chip-info --chip STM32H725IG` fills it). Without it, the chip checks are skipped with a note.

stm32-data's list of a chip's pins leaves out H7's analog-only pads (`PA0_C`); the pins its peripherals name are added.
