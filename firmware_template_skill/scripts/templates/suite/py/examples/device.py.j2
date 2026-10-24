"""Read everything the board's device contract serves: board, ids, chip, counters, log, settings, boot state and
the update node. From the repo root: `just py-example device [PROBE_SERIAL]`, or
`uv run --project nucleo_g0b1re_py python nucleo_g0b1re_py/examples/device.py [PROBE_SERIAL]`. Counters and the
log are decoded with the firmware's ELF: `just flash` or `just run` puts it where it is found by its build id."""

import sys

import nucleo_g0b1re

serial = sys.argv[1] if len(sys.argv) > 1 else None
with nucleo_g0b1re.Device(probe_serial=serial) as dev:
    print("board:", dev.board())
    print("uid:", dev.uid())
    print("variant:", dev.variant())
    print("last reset:", dev.last_reset())
    print("counters:", dev.counters())
    print("led at boot:", dev.led_at_boot)
    print("boot state:", dev.boot_state())
    print(dev.dfu_node())
    print("last log lines:")
    for line in dev.log()[-5:]:
        print("  ", line)
