# Blink codes

A firmware that won't start says why on its status LED, the same way on every bedrock board (`bedrock_led::code`, DEV-18). It does so first thing in `main`, before anything on the board is set up, then reboots into the chip's ROM bootloader (RP2350 BOOTSEL, STM32 system DFU), where `ww` can read the board and flash it. A reset or a power cycle repeats it.

## Reading a code

One long flash (1 s), then a number of short flashes (0.25 s, 0.4 s apart), then 1.5 s dark; the code starts again. It goes on for about 10 s, then the LED goes dark and the board shows up as a USB boot ROM device (`ww dfu rp list`, `ww dfu stm32 list`). On an RGB LED the flashes are red.

| Short flashes | Code | What it means | What to do |
|---|---|---|---|
| 1 | `UnknownRework` | The board record has a rework applied that this firmware doesn't know, and no allowance for this very image covers it. Any init could damage the reworked board, so nothing is set up. | Flash a firmware that knows the rework. Or, if this image is right for the board as it is, allow it: `ww board rework allow N` while the board is in the ROM bootloader (`ww info` there lists the unknown numbers). The next `ww` flashing clears the allowance. |
| 2 | `RecordUnreadable` | The board record can't be read: from a newer firmware's format, damaged, or the store doesn't fit what the image expects. The firmware can't tell what the board is. | `ww info` in the ROM bootloader says what it found; flash the firmware that wrote the record, or rewrite the record from a firmware that can. |

A new reason takes the next number; numbers are never reused.

## On a board

The board drives the LED pin as a plain GPIO output, no PWM, no timer, and waits with a busy loop on the clock the ROM left: the timing is for a person, not exact. A board whose status LED sits on a net a rework may change has no LED to blink and goes straight to the ROM bootloader.

```rust,ignore
use bedrock_led::code::{blink, Code};

if !guard.runs_board() {
    blink(Code::UnknownRework, |on| led_pin_set(on), |ms| cortex_m::asm::delay(ms * BOOT_CLOCK_KHZ));
    reboot_into_rom_bootloader();
}
```
