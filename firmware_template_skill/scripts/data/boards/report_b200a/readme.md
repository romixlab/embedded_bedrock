## The board

![rePort top view: the headers' pins, coloured by what they are for](docs/board/report_b200a-top.svg)

The B200A rePort is a USB-to-anything adapter: an RP2350B (QFN-80) with 512 KiB of QSPI flash (BY25Q40), USB FS on
a USB-C connector, and behind level translators on the 2×15 header J102: eight IO channels (IO0-IO7), UART, SPI,
I2C, an adapter detect pin and eight ADC channels through an op-amp buffer. A chain of four SN74LVC595 shift
registers (32 outputs, EX0-EX31) switches the translators, the IO level `+VREF` (5 V, 3.3 V, "1.8 V" or an external
level on J103 pin 7) and the pull-ups and pull-downs. [docs/board/](docs/board/) has the bottom view, the power tree
and [the pin table](docs/board/report_b200a-pins.md) with a note per pin; `just board` re-draws them from
`docs/board/report_b200a.toml` with [boardmap](https://github.com/romixlab/embedded_bedrock) (bedrock_board).
The board's hardware reference is `doc/hw_b200a.md` in its [hardware repository](https://github.com/vhrdtech/hw/tree/main/b200_re_port).

**The 595 chain first:** on rev A `EXP.~OE` (GP27) has no pull-up, so from power-on the chain's outputs drive
whatever its registers hold, which can turn on two `+VREF` sources at once and short VBUS into the 3.3 V rail.
`main` drives `~OE` high, shifts in the safe word (everything off: all zeros but EX13, the 3.3 V source, which is
active low), latches it and enables the outputs again, right after `embassy_rp::init`, before anything else. The
firmware never touches the chain afterwards: the translators, `+VREF` and the pulls stay off, so every pin behind
them is an input or high-Z on both sides. usb_io_fw's `b200_board/src/chain.rs` has the rules for switching them
(at most one `+VREF` source, one I2C path, one SPI translator).

**Power:** from USB-C only (VBUS `+5V0_USBIN`, a sink with 5k1 on CC). The RT5760 makes 3.3 V, the TLV74311 the
1.1 V core supply (the RP2350's own regulator is unused). The "1V8" LDO U1602 is a TLV70233, a 3.3 V part, on the
boards as built: every "1.8 V" level is about 3.2 V until it is swapped.

**The probe:** SWD on J101 (2×3): 1 SWDIO, 3 SWCLK, 5 reset, 6 3V3 (the probe's target voltage), 2 and 4 GND.

**D102, the user LED:** 3.3 V through the LED and 33 Ω into GP38, so low lights it. GP38 is PWM slice 11 A, so the
firmware breathes it.

**SW101, the user button:** GP22 to 3.3 V when pressed, with no pull-down on the board: the firmware uses the
internal one. RP2350-E9 (RP2350 datasheet, "Increased leakage current on Bank 0 GPIO when pad input is enabled",
A2 stepping; fixed in A3) can hold such a pin high after release, so the firmware enables the pad's input buffer
only for each read. The BOM orders the A4 stepping, which `{cli} device` shows as `variant`; the read works on
every stepping. Rev B adds the external pull-down.

There is no BOOT button: BOOTSEL is J103 pin 5 ("BOOTSEL by hand" under [Device](#device)).
