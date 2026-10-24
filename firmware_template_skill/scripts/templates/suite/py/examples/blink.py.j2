"""Blink the Nucleo's LED and print what runs on it. From the repo root: `just py-example`, or
`uv run --project nucleo_g0b1re_py python nucleo_g0b1re_py/examples/blink.py [PROBE_SERIAL]`."""

import sys

import nucleo_g0b1re

serial = sys.argv[1] if len(sys.argv) > 1 else None
with nucleo_g0b1re.Device(probe_serial=serial) as dev:
    print(dev.api())
    print(dev.firmware())
    print("label:", dev.label or "(none)")
    print("boot count:", dev.boot_count())
    dev.blink(count=3, period_ms=400)
