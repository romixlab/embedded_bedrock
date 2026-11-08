# 🔗 USB — USB device side

Detail for the `USB` checks of the [check catalogue](../checks.md).

## <a id="usb-1"></a>`USB-1` — high-speed packet size and bInterval out of range

A High-Speed bulk endpoint must advertise a 512-byte max packet size; a Full-Speed value (64) left over from
porting an example, or a `bInterval` the speed doesn't allow, can work on one host's driver stack and misbehave
on another's.

**Example:** vb135's USB-HS serial example needed exactly this fix on Windows (commit `c0d19b8`): "Fix USB HS
example on Windows (needed to use 512B packet size). bInterval is also incorrectly set to 255 (reported in Linux
as 11 max, though seem to work)." Linux's USB stack tolerated the wrong values; Windows did not.

**Fix:** set the max packet size from the negotiated speed (512 for HS, 64 for FS) rather than a literal copied
from an example, and keep `bInterval` inside the range the USB spec allows for the endpoint's speed and
transfer type — check against a second host stack (Windows and Linux differ here), not just the one in daily
use.

**Detection:** src mid — the descriptor values are static and the speed/type combination is known at build
time, so a lint can check max packet size and bInterval range per advertised speed.

## <a id="usb-2"></a>`USB-2` — a short-packet framing assumption the transfer doesn't meet

Two related traps: sending a transfer that is an exact multiple of the endpoint's max packet size without a
zero-length packet leaves the host's USB stack waiting for one more packet to know the transfer ended (it only
knows a bulk transfer is done on a short packet or a ZLP); and framing a request's end on "the next short
packet" breaks when the host happens to send an exact-multiple-sized request and no ZLP follows it.

**Example:** typec_dongle's CMSIS-DAP framing (FW-19) hit the second shape directly: probe-rs 0.32 sends no
zero-length packet after a request that is an exact multiple of 64 bytes (25 batched register writes come to
exactly 128 bytes), so a request end detected by "waited for a short packet or a pause" cannot tell a slow host
from a finished exact-multiple request. The fix parses each command's own length from its CMSIS-DAP layout
instead of inferring the end from packet framing at all.

**Fix:** know the transfer's length from the protocol itself (a length field, a fixed command size) rather than
inferring "done" from packet shape; when sending, add the ZLP whenever the payload is an exact multiple of
the max packet size.

**Detection:** review — needs the actual protocol framing on both the send and receive side; a source lint
can't tell whether a given transfer size is "common" for the traffic this endpoint carries.

## <a id="usb-3"></a>`USB-3` — single-buffered bulk endpoints behind a hub TT

A single-buffered bulk endpoint can't accept (or supply) the next packet while firmware is still handling the
previous one, so there is dead time on the wire between packets; through a hub's Transaction Translator (TT)
that dead time is far more expensive than directly on the host's own root port, because the TT serializes
Full-Speed traffic from multiple downstream devices.

**Example:** typec_dongle's WireWeaver USB throughput (FW-24, on the G0's `usb_v4` peripheral) went from 373 to
664-676 kB/s host→device purely from switching to double-buffered bulk endpoints (`embassy-rs/embassy#7275`),
alongside `opt-level = 3` and a protocol fast path — nearly double, from firmware-side buffering changes alone,
with the same wire protocol and the same host.

**Fix:** double-buffer (or use more than one buffer per direction) any bulk endpoint on a path that matters for
throughput, especially one a hub TT sits in front of; chip: STM32 `usb_v4` parts (G0, among others) need this
explicitly since the peripheral itself only gives one buffer per endpoint direction unless the double-buffer
feature is used.

**Detection:** src easy — whether an endpoint is constructed with single or double buffering is a static
property of the driver call, chip: STM32 `usb_v4`.

## <a id="usb-4"></a>`USB-4` — sending gets stuck when the host side goes away without a clean disconnect

A bulk IN endpoint that the firmware keeps trying to send on can wedge when the host's driver closes abruptly
or the host itself dies, without the USB-level disconnect the firmware's state machine was written to expect —
the endpoint NAKs forever and nothing unwedges it from the firmware side alone.

**Example:** vb135's fix (commit `bb820d9`, "fix: stuck at sending Ping when host driver closed") is exactly
this: the firmware kept trying to send a Ping frame on an endpoint the host had abandoned without a proper USB
disconnect, and had no path to notice and recover. typec_dongle's FW-26 is the same class of problem on its own
sending path.

**Fix:** bound every send attempt with a timeout independent of USB-level disconnect events, and give the
sending task a way to be told "give up and reset the endpoint state" from outside the send itself (a watchdog
noticing no progress, a host-liveness signal) rather than relying on the disconnect callback alone.

**Detection:** bench — this needs an actual host that kills its USB driver mid-transfer (unplugging is not the
same event), so it stays a bring-up/bench check rather than something source analysis can provoke.

## <a id="usb-5"></a>`USB-5` — an OUT packet read before the USB SRAM write is done

On some STM32 USB device peripherals the CTR interrupt of an OUT transaction fires before the peripheral has
finished writing the packet and its byte count to USB SRAM. Firmware that reads the buffer straight away can get
the previous packet's tail or a stale count.

**Erratum:** ES0548 Rev 4 (STM32G0B1xB/xC/xE) §2.16.1 "Buffer description table update completes after CTR
interrupt triggers", every silicon revision (A, Z/3, Y/4): "During OUT transfers, the correct transfer interrupt
(CTR) is triggered a little before the last USB SRAM accesses have completed. If the software responds quickly to
the interrupt, the full buffer contents may not be correct." Workaround: "Software should ensure that a small
delay is included before accessing the SRAM contents. This delay should be 800 ns in Full Speed mode and 6.4 μs in
Low Speed mode." STM32H523/533 (ES0621 Rev 3 §2.19.1) and STM32U3 (ES0626 §2.18.1) have the same erratum.

**Example:** embassy-stm32's `usb` driver waits 1 µs before reading an OUT packet only under
`#[cfg(any(stm32h5, stm32u3))]` (`wait_for_out_packet_memory()`, embassy main 97c4d7e8a, 22 Oct 2026), not on the
G0B1. Waking a task on the interrupt usually takes longer than 800 ns at 64 MHz (52 cycles), but a task that is
polled for another reason (a `select` with a timer) can see the endpoint's STATRX at NAK right after the transfer
and read too early. Found planning bedrock_usb (embedded_bedrock USBD-3), not yet seen on a board.

**Fix:** wait at least 800 ns (full speed) between seeing the packet (STATRX NAK, or DTOG and SW_BUF differing on
a double-buffered endpoint) and reading its count or data, on every part whose errata sheet lists this erratum.
bedrock_usb's usb_v4 driver waits `v4::OUT_PACKET_DELAY_NS` before every OUT read, SETUP included; its simulated
peripheral flags an SRAM read that comes sooner.

**Detection:** review — the read path and the part's errata sheet; a source lint can't tell how long the path
from the interrupt to the read is.
