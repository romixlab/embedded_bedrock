## The board

![Nucleo-G0B1RE top view: every header pin, coloured by what it is for](docs/board/nucleo_g0b1re-top.svg)

Green pins are free, orange ones have a job on the board by default that a solder bridge removes (PA2/PA3 go to
the ST-LINK's virtual COM port, PA5 drives LD4, PC13 is the USER button, PF0 carries the ST-LINK's 8 MHz MCO), red
ones are the debug port and reset. [docs/board/](docs/board/) has the bottom view with every solder bridge and its
default, the power tree (USB, VIN, E5V, the JP2 selection, the regulators) and
[the pin table](docs/board/nucleo_g0b1re-pins.md) with a note per pin; `just board` re-draws them from
`docs/board/nucleo_g0b1re.toml` with [boardmap](https://github.com/romixlab/embedded_bedrock) (bedrock_board).

**B1, the user button:** PC13, pulled up to VDD through R29 (reads `1`) and pulled to GND when B1 is pressed
(reads `0`); SB20 (ON by default) ties B1 to PC13, OFF frees the pin (UM2324 §6.11; R29/SB20 nets from the
MB1360-G0B1RE-C02 schematic).

**LD4, the user LED:** PA5 drives it through a MOSFET (T2), high lights it, SB22 (ON by default) wires PA5 to
it (UM2324 §6.10 point 4). PA5 is also `TIM2_CH1_ETR` (alternate function 2, STM32G0B1xx datasheet DS13560): a
PWM there can breathe the LED instead of a plain on/off GPIO. **LD1 (COM)** is driven by the ST-LINK/V2-1
itself, not the target MCU, and blinks red/green to show USB enumeration and PC↔ST-LINK↔STM32 communication
state; **LD3 (PWR)** is on whenever the board's 5 V rail is present (UM2324 §6.10 points 1 and 3).

**CN4, the SWD target jumpers:** both ON (default) route the ST-LINK/V2-1's SWD to the on-board STM32G0B1RE;
both OFF route it out to CN11 instead, to program or debug an external target board (UM2324 §6.4.3, §6.4.4,
Table 6). Don't wire CN11 while CN4 is ON, it can disturb the on-board link.

