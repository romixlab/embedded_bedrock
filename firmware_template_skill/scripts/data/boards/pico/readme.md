## The board

![Pico top view: every header pin, coloured by what it is for](docs/board/pico-top.svg)

The Raspberry Pi Pico (R3) carries an RP2040, a 2 MiB W25Q16JVUXIQ QSPI flash chip and a 12 MHz crystal; USB FS on
a micro-B connector is both its power and its only link to a PC. 26 of its 30 GPIOs (GP0-22, GP26-28) reach the two
edges as castellated pads; green ones in [docs/board/](docs/board/) are free, red the LED and the debug-adjacent
pins, orange the 3V3 enable, grey a supply or a fixed pin. [The pin table](docs/board/pico-pins.md) has a note per
pin; `just board` re-draws the pictures from `docs/board/pico.toml` with
[boardmap](https://github.com/romixlab/embedded_bedrock) (bedrock_board).

**Power:** from the USB connector's VBUS, through a Schottky diode onto VSYS, or straight onto VSYS from its header
pin for a battery (1.8-5.5 V) without USB; [the power tree](docs/board/pico-power.svg) has both paths. The on-board
RT6150B-33 (U2) makes 3V3 from VSYS, switching or linear depending on load. GP24 reads VBUS through a divider (high
while USB powers the board) and GP29 reads VSYS / 3 on ADC3, gated by a FET so sampling it is the only time it
draws current; neither is on a header.

**The probe:** a Raspberry Pi Debug Probe (2e8a:000c) on the 3-pin SWD header (SWCLK, GND, SWDIO; unpopulated,
near the top of the board between the two GPIO edges), which also carries the defmt log over SWD or RTT. There is
no reset button and no NRST pin broken out: a reset is a power cycle (unplug / plug the USB cable back in) or the
Debug Probe's own `probe-rs reset`.

**The LED and BOOTSEL:** GP25 drives the only LED, high lights it, and it breathes on PWM slice 4 channel B
(`bedrock_led`). There is no user button: SW1 (BOOTSEL) grounds the flash's chip-select at power-on instead of
wiring to a GPIO, which only the boot ROM reads, so firmware never sees it pressed.

**Not covered:** the Pico W replaces GP23/24/25/29's wiring with a CYW43439 (Wi-Fi/BT) on SPI, and its LED moves
onto the CYW43's own GPIO, driven over that SPI link rather than a plain GPIO; TPL-62 is the plain Pico (R3) only.
