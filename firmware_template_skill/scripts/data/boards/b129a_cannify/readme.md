## The board

![CANnify Micro top view: every header pin, coloured by what it is for](docs/board/b129a_cannify-top.svg)

The B129A CANnify Micro HV Pico-Lock is a one-channel CAN FD node: an STM32G0B1CET6 (LQFP48), a TCAN1044V
transceiver on FDCAN1, USB FS on a USB-C connector and two Pico-Lock connectors that carry the bus and its supply
(1 V+, 2 CANH, 3 CANL, 4 GND). Green pins on the two 1x20 headers J1 and J2 are free, red ones are the debug port,
reset and the supplies. [docs/board/](docs/board/) has the bottom view, the power tree and
[the pin table](docs/board/b129a_cannify-pins.md) with a note per pin; `just board` re-draws them from
`docs/board/b129a_cannify.toml` with [boardmap](https://github.com/romixlab/embedded_bedrock) (bedrock_board).

**Power:** from USB-C (VBUS through U4, an AOZ1376 with 5.8 V over-voltage cut-off and reverse blocking) or from the
bus's V+ on J4 / J5 (up to 53 V, F1 4 A, then the LV14360 buck to 5 V); the RT5760 makes 3.3 V from the 5 V. Either
source alone runs the board. PB2 reads VBUS through a ÷ 10.6 divider and PB10 the bus supply +VIN_CAN through ÷ 24.2
(ADC1_IN10 and IN11).

**The probe:** SWD on J2: GND J2-8, SWCLK J2-9, SWDIO J2-10, 3V3 J2-11 (the probe's target voltage); reset is J1-6.
PA14 (SWCLK) is BOOT0 as well and R28 pulls it low, so the chip boots from flash. With a USB-C debug accessory (both
CC lines pulled up), U7 routes SWD to the connector's SBU pins instead.

**CAN:** FDCAN1 TX on PD1, RX on **PB8** (the board's CubeMX file says PD0: the schematic and the board have PB8),
the transceiver's STB on PC13: low for normal mode, high or floating leaves it in standby, and there is no resistor
on it. The clock tree runs the FDCAN kernel from the 12 MHz crystal. The firmware does not drive CAN yet.

**D14, the user LED:** blue, PB14 drives it through R27 (1k69), high lights it. PB14 is `TIM15_CH1` (alternate
function 5, STM32G0B1xx datasheet DS13560), so a PWM breathes it. The board has no button and no power LED.

PA4 and PA5 are the only I/O pins that are not 5 V tolerant (the schematic's note).
