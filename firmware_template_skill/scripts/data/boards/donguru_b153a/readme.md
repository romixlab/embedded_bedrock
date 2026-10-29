## The board

![Donguru top view: the header J101 pin by pin, coloured by what it is for](docs/board/donguru_b153a-top.svg)

The B153A Donguru sits between a PC and a USB-C device: the receptacle J401 goes to the PC, the plug J501 to the
device under test. An STM32G0B1CET6 (LQFP48) runs it: a USB2512B hub (U301) passes the device's USB through on
its port 2 and carries the MCU's own USB on port 1, U601 switches VBUS to the device and PB0 measures its current,
UCPD1 / UCPD2 reach both CC sides, and the plug's SBU pins go to the MCU (directly on PA0 / PA1, or through the
TXS0102 level shifter on PB6..PB11). The header J101 has four protected I/O lines, 3.3 V, the switched receptacle
VBUS and a photo-MOS relay. [docs/board/](docs/board/) has the bottom view, the power tree and
[the pin table](docs/board/donguru_b153a-pins.md) with a note per pin; `just board` re-draws them from
`docs/board/donguru_b153a.toml` with [boardmap](https://github.com/romixlab/embedded_bedrock) (bedrock_board).

**Rev A as built differs from its schematic** (hw/b153_typec_serial/doc/hw_b153a.md §0): the CC / SBU
over-voltage protection (U402, U501) is removed and bridged, so nothing protects those pins above 5 V; a 5K1 Rd on
the receptacle's CC lets a USB-C PC power it with an empty MCU; the hub's RESET_N is a wire to PC13, cut from NRST;
the debug-accessory Rp (PA15) is cut. Never drive PF2 (NRST), PC6 (tied to GND) or PC7 (tied to 3.3 V).

**The hub:** U301 waits in SMBus configuration mode until the MCU writes its config over I2C2 (PB10 / PB14,
address 0x2C, the SDA pull-up fed from PA4) and attaches it. `src/main.rs` does that before USB starts: self-powered,
port 2's D+ / D- swapped (crossed on the board). PC13 holds the hub's reset through RTC_OUT1, so the hub and the
PC's view of it survive a reset of the MCU (and the jump into the ROM bootloader); a power cycle starts it over.

**Power:** from either VBUS (diode-OR D801 / D802, then the AP64060 buck to 3.3 V), so the board runs from the PC
or from a powered device. PA3 reads the receptacle's VBUS (÷ 7.17). PA6 turns the plug's VBUS on, PD1 puts the
receptacle's VBUS on J101-10, up to 20 V when the PC negotiated it: the template leaves both off (their pull-downs
hold them so).

**The probe:** SWD is on the receptacle's SBU pins: SWCLK (PA14) on SBU1, SWDIO (PA13) on SBU2, also on TP202 /
TP201. A J-Link through a USB-C-to-SWD breakout, or another Donguru as the SWD host, reaches it. PA14 is BOOT0 as
well, pulled low by R209.

**The LEDs:** LD101 red on PF0 (TIM14_CH1), LD102 green on PF1 (TIM15_CH1N), high lights them; the firmware
breathes them with PWM (`src/led.rs`) and moves embassy's time driver to TIM4.

**The button and the switch:** both on PA7, an analog pin (R101 10K up): idle 3.3 V, the slide switch SW101 2.89 V,
the button SW102 1.65 V, both 1.54 V, read with ADC1_IN7. There is no digital button, so the template has none.
The button held at reset pulls BOOT0 high through Q204: the chip starts the ROM bootloader, if the option bytes
let the BOOT0 pin decide (nBOOT_SEL); on rev A its USB stays dark then, since the hub is held in reset until a
firmware releases it (README "The ROM bootloader").
