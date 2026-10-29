## The board

![B135B top view: the J203 header pins, coloured by what they are for](docs/board/b135b-top.svg)

The B135B FDCAN ISO USB is an isolated USB high speed to CAN FD adapter: an STM32H725IGK6 (TFBGA176+25, 1 MiB flash
in one bank) at 520 MHz, USB 2.0 high speed on a USB-C connector through a USB3315 ULPI PHY, one CAN FD channel
through the isolated ADM3050E transceiver on a DE-9 with a switchable 120 Ω termination, a second CAN FD channel
as bare logic on the 2x4 header J203, a microSD slot, a BOOT0 button and an RGB LED.
[docs/board/](docs/board/) has the bottom view, the power tree and [the pin table](docs/board/b135b-pins.md) with a
note per pin; `just board` re-draws them from `docs/board/b135b.toml` with
[boardmap](https://github.com/romixlab/embedded_bedrock) (bedrock_board). Of the MCU's 176 balls few are wired:
the pin table lists the ones that are not.

**Power:** from USB-C (VBUS through U401, an AOZ1376 over-voltage and reverse current switch) or from the bus's V+
on DE-9 pin 9 (D504 TVS 58 V, F601 1 A, then the LT8304 isolated flyback U601), either onto +5V0_SYS; the RT5760
U402 makes 3.3 V from it, the NCP563 U302 1.8 V for the USB3315, and the 1 W module PS501 the isolated 5 V of the
CAN side. The core supply is the chip's own SMPS alone (`SupplyConfig::DirectSMPS`: VLXSMPS feeds VDDLDO, VCAP and
VFBSMPS on one net). PC5 reads +5V0_SYS through a ÷ 2 divider, PA9 senses VBUS.

**Clocks:** the 24 MHz oscillator X301 drives PH0 (HSE in bypass) and the USB3315's REFCLK; PLL1 makes the 520 MHz
core clock (voltage scale 0), PLL2 Q 80 MHz for FDCAN, PLL3 Q 48 MHz for the USB peripheral. The 32.768 kHz crystal
X201 is the LSE, VBAT comes from a CR1632 in B201.

**The probe:** SWD on the Tag-Connect J202 (TC2050, 10 pins): 1 VTref (through R217 49R9), 2 SWDIO, 4 SWCLK,
8 BOOT0, 10 NRST, 3, 5 and 9 GND. A SEGGER J-Link (`1366:0101`) with a TC2050-IDC cable; probe-rs reaches it as
any other probe.

**USB:** the TS3USB221A U301 switches J301 between the USB3315 (PH5 low: high speed through USB_OTG_HS's ULPI)
and PA11 / PA12 (PH5 high, where R305 holds it at reset: full speed, the ROM bootloader's DFU). The firmware drives
PH5 low and the USB3315's RESETB (PH3, R312 pulls it low) high before it starts USB.

**CAN:** FDCAN1 TX on PB9, RX on PB8 to the ADM3050E U501 (isolated, no standby pin), a common-mode choke and ESD
diodes to the DE-9 J501 (2 CAN_L, 7 CAN_H, 3 and 6 isolated GND, 9 bus V+, 5 the shield through JP501). The 120 Ω
termination (R506 / R507 2 × 60R split by C515) sits behind the latching relay K501: a pulse high of 20 ms on PC13
(TERM_EN, through the TLP2270 U502) switches it on, one on PE4 (TERM_DIS) off, and the relay keeps its state
without power. The firmware leaves both pins low and does not switch the termination yet: `can[]` has no
terminator to set. FDCAN3 TX on PG9, RX on PD12 come out bare on J203-3 / J203-4 (3.3 V logic, add a transceiver).
On the board PG9 is wired to PB3 and PD12 to PA7: those two stay inputs. The FDCAN kernel clock is PLL2 Q, 80 MHz.

**ES0491 (STM32H72x/73x errata, Rev 9) on this board:** the FDCAN ones apply to both channels. 2.22.1
*Desynchronization under specific condition with edge filtering enabled*: "Disable edge filtering or wait for frame
retransmission"; bedrock_can leaves edge filtering off unless a host turns it on. 2.22.2 *Tx FIFO messages inverted
under specific buffer usage and priority setting* needs a dedicated Tx buffer beside the FIFO; the firmware sends
from the Tx FIFO alone, so it is not affected. 2.22.3 *DAR mode transmission failure due to lost arbitration*: in
one-shot (DAR) mode a frame may fail on lost arbitration in the first two identifier bits; the workaround (clear
TRPx, set CFx, send again) is not in the firmware, so with one-shot on such a frame is reported as not sent. 2.2.5
*USB DFU is not functional when the device is readout protected (RDP level 1)*: with RDP 1 set, the ROM bootloader
cannot update the board over USB; "A device under RDP level 1 protection can be reinitialized to RDP level 0 by
any means except USB DFU", here the J-Link. 2.2.6 (LSE disturbed by PC13 toggling) is for LQFP packages only: "The
WLCSP TFBGA and UFBGA packages are not impacted", so the termination pulses on PC13 leave X201 alone.

**D201, the RGB LED:** common anode on +3V3, red on PC7 (R202 130R), green on PC6 (R203 33R), blue on PH13 (R204
33R): low lights a colour. PC7 / PC6 are TIM3 CH2 / CH1 and PH13 is TIM8_CH1N, so PWM breathes them.

**SW201** pulls BOOT0 to 3.3 V (R220 10k pulls it down); PE1 reads the same net, so the firmware sees the button
as `sw201`. Held while the board powers up or resets, it starts the ROM bootloader.

Free for an application are PE0 (J203-5), PB4 (J203-7, wired to PB7 on the board, which stays an input) and PA8
(MCO1, on the test point TP202). J203-1 / J203-2 are PA1_C / PA0_C, the analog-only pads; J203-8 is +5V0_SYS,
J203-6 GND.
