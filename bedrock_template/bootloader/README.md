# Bootloader for {{project-name}}

The bootloader uses `embassy-boot` to interact with the flash.

# Usage

Flash the bootloader, cargo flash will reset the MCU afterwards, MCU will end up in lockup state because no application is there yet.

```
cargo flash --release --chip {{probe_chip}}
```
