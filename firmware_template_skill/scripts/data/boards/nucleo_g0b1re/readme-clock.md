`{cli} clock` prints the board's `Locked` / `Acquiring` state, the **residual** (the board's shared time minus the
PC's UTC, read once in the middle of a round trip, so known only to within half of it), the offset the board still
slews in, the error bound the host sent with its last steer point and the fit's rate (how far the board's HSI16-based
clock runs from the PC's):

```text
Locked (Host), residual -0.70 ms ± 4.28 ms, slew 0 µs, error ≤ 4.55 ms, rate -3435.5 ppm, fit rms 0.53 ms
```

**How close it is, over RTT.** The host reaches the board by polling RTT through the ST-LINK, so a round trip takes
milliseconds, and that is the limit. Measured on one Nucleo for 5 minutes (`just py-example clock 300`, a check a
second, 24 Oct 2026):

| | |
|---|---|
| state | `Locked` in all 300 checks |
| residual | mean −0.23 ms, standard deviation 1.10 ms, largest 3.8 ms |
| round trip | median 8.2 ms, longest 20.7 ms; each residual is known only to ± half of it |
| error bound the host sends | 4.2–5.5 ms (half the shortest round trip plus twice the fit's scatter) |
| board clock rate | −3390 to −3480 ppm against the PC (the HSI16, uncalibrated) |

So the board follows UTC to a few milliseconds; the scatter of the residual is mostly the round trip, not the board.
Because the error bound is about 4 ms, the firmware locks within 10 ms (`firmware/src/sync.rs`) instead of
bedrock_sync's 2 ms, which suits a USB board with sub-millisecond round trips. For a breath of 3 s, boards a few
milliseconds apart look in phase. Two boards side by side were not tried yet.
