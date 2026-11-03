# 🛡️ HOST — Host input and the device API

Detail for the `HOST` checks of the [check catalogue](../checks.md).

## <a id="host-1"></a>`HOST-1` — a panic the host can trigger from a request

Any value that can reach `unwrap`, `expect`, slice indexing, `unreachable!` or an arithmetic op that can
overflow, and that traces back to host input (a pin number, a length, a rate, a count field), is a
denial-of-service the host side doesn't even need to be malicious to hit — a buggy or older host sending a
malformed or truncated request is enough.

**Example:** typec_dongle's CMSIS-DAP path (FW-19) had exactly this: `dap_rs` indexes past the end of a
truncated request, so a request cut short by the host panicked the firmware. usb_io_fw's `AGENTS.md` states the
rule plainly: "No panics on host input: every value from the API is validated (pin numbers, voltage levels,
baud rates) and rejected with an error the host can show."

**Fix:** validate every host-supplied value at the boundary and return a typed error instead of indexing,
unwrapping or computing on it directly; for a request whose own framing can be short (FW-19's fix), check the
declared length against what actually arrived before parsing it as if it were complete.

**Detection:** src hard — finding every path from a host-controlled value to a panicking operation needs the
call graph, not just a pattern match; `check: src hard` in the catalogue reflects that.

## <a id="host-2"></a>`HOST-2` — the host can ask for something that damages the board

A host request that drives a pin, a level translator direction, or a supply switch is, from the firmware's
point of view, just another API call — but on the board it can be a short, a back-feed into an unpowered
device, or over-voltage on a part rated for less. The firmware is the last line that can refuse it.

**Example:** usb_io_fw's `AGENTS.md` names this directly as "Electrical safety": "the firmware must never drive
a pin into a voltage level or mode the board can be damaged by (wrong level shifter direction, Ext above 3V3 on
I2C)," with the schematic as the source of truth for what's actually dangerous on a given board.

**Fix:** enumerate the board's real electrical rules (which pins can't be driven together, which levels a rail
tolerates, which translator directions are safe when) and enforce every one of them in the handler that could
violate it, refusing with an error rather than performing the request.

**Detection:** review — this needs the schematic, not source analysis; a lint can at best flag a handler that
drives a pin without going through a shared "is this safe" gate once such a gate exists.

## <a id="host-3"></a>`HOST-3` — an unknown enum value silently becomes a default

A host built against a newer wire protocol can send an enum variant this firmware doesn't know about. Matching
it with a catch-all arm (`_ => some_default`) makes the firmware silently *do something else* than what the
host asked for, instead of telling the host its request wasn't understood.

**Example:** this is a general wire-compatibility trap anywhere a request carries an enum the firmware
dispatches on — usb_io_fw's diag API (API-5) is exactly the shape of surface (many small enum-tagged requests,
built and evolved independently of any one host) where a newer host's added variant would otherwise land on
whatever arm a `_ =>` catch-all happens to pick.

**Fix:** refuse an enum value the firmware doesn't recognize with a typed "unknown/unsupported" error instead of
mapping it to any existing variant; WireWeaver's evolution rules exist so a protocol can grow a variant without
old firmware needing to guess at it.

**Detection:** src mid — needs the dispatch model (which match arms exist, which is the catch-all, whether the
catch-all returns an error or a default) rather than a single pattern.

## <a id="host-4"></a>`HOST-4` — a host that stops reading can stall a release or cleanup path

A release, disconnect or cleanup path that waits on the host to read something it sent — directly or
indirectly, by waiting on a buffer the host's read would free — can hang forever if the host has simply stopped
reading (closed its handle, crashed, got stuck). Every such wait needs its own way out.

**Example:** typec_dongle's CMSIS-DAP responder (FW-19) gives the host 1 second to read a response
(counted as `dap_response_timeouts`) and then goes back to listening regardless, specifically so that
`sbu.release` — the cleanup path that must run on disconnect — "is never stuck behind a host that stopped
reading."

**Fix:** put a timeout on every wait that depends on the host doing something, and make sure the cleanup/release
path the timeout feeds into actually runs to completion rather than just logging the timeout and continuing to
wait.

**Detection:** review — needs tracing which waits are host-dependent and whether a cleanup path is reachable
without the host's cooperation; `check: review` in the catalogue.

## <a id="host-5"></a>`HOST-5` — truncated and oversized requests that aren't counted

A request shorter than its own command needs, or a response the firmware is asked to produce that is larger
than the protocol's packet limit, has to be refused — but if it's refused silently, a host sending malformed
traffic (an older host against a newer firmware, or a genuinely buggy one) leaves no trace to diagnose with.

**Example:** typec_dongle's CMSIS-DAP path (FW-19) counts every one of these distinctly: `dap_requests_truncated`
for a request cut short, `dap_requests_too_long` for one longer than the 512-byte packet limit, and
`dap_responses_too_long` for a response that would exceed it — each dropped without a panic, but each counted
so the pattern is visible without a debugger attached.

**Fix:** give every "this request/response doesn't fit the protocol" rejection its own `cnt!` counter (CNT-1),
distinct from a generic error count, so a host sending malformed traffic shows up in `device.counters` instead
of silently failing.

**Detection:** src mid — a lint can check that a request-validation early-return path reaches a `cnt!` call,
though telling truncated from oversized from malformed needs the handler's own branching.

## <a id="host-6"></a>`HOST-6` — a host-settable range the firmware can't actually serve everywhere

A parameter the host is allowed to set (a sample window, a buffer size, a rate) can have a documented range in
the API that is wider than what the firmware can actually deliver, because the real limit comes from DMA buffer
geometry or similar firmware-internal constants that the API's declared bounds don't automatically track.

**Example:** typec_dongle's power meter (API-12) has exactly this kind of derived bound: its `window` parameter
is valid from 0.9 to 14.9 ms, a range set by "one sequence up to one half DMA buffer of sequences" — a value
outside that isn't a protocol violation, it's a value the DMA buffer physically can't produce, and sampling
longer windows needs averaging across updates instead of one bigger window.

**Fix:** derive the API's declared bounds from the same constants that bound the implementation (the DMA buffer
size, the hardware FIFO depth) instead of hand-picking a range that happens to look right; when the two drift
apart, the host can ask for something the firmware will reject or truncate without being able to predict it.

**Detection:** review — the two bounds (API-declared, implementation-derived) live in different places and
need a human to check they still agree whenever either changes.

## <a id="host-7"></a>`HOST-7` — one unimplemented field voids a whole batched update, or an answer only fires from inside the handler

Two related traps in a batched or deferred API: a batched update where one field the firmware doesn't implement
yet makes the *whole* update fail, instead of applying what it can and reporting only that field as
unimplemented; and a deferred answer (a credit grant, a completion) that is only ever sent from code running
inside the request handler that triggered it, so any other reason the answer becomes ready (a background task
finishing, a timer) has no path to actually send it.

**Example:** WireWeaver's shared observe-loop work (SRV-24) rebuilt `poll_properties` as one loop over
observation levels specifically so a value that can't be had at a given write time is dropped and counted as
*missed* on its own, while the rest of the batched update still goes out — the earlier shape risked one
unavailable property blocking everything else in the same batch. usb_io_fw's shared SD server (IF-13) is built
around deferred methods (`claim`, `write`, `erase`, ...) that are answered later with `send_return`, which only
works if the answer path is reachable from outside the originating handler — exactly the shape this check
exists to make sure doesn't regress.

**Fix:** make a batched update's per-field failures independent (each field succeeds, fails or is
"unimplemented" on its own, not the whole batch), and make sure a deferred answer can be sent from whatever
code actually learns the result, not only from inside the handler that received the original request.

**Detection:** test — exercising "one field fails, does the rest of the batch still apply" and "the answer
arrives from outside the handler" needs an actual round trip, not source inspection.

## <a id="host-8"></a>`HOST-8` — loss reported out of order with the data it describes

When a transport can drop data (a UART capture, a streamed log) and separately reports "N bytes were lost here,"
that loss report has to arrive in the same order relative to the data it's about — a loss notification that
shows up before or after the data stream has already moved past the point it describes is worse than useless,
since the host can't tell what it actually lost.

**Example:** typec_dongle's `uart[1].rx` capture path (0.20.14, building on the LZ4-compressed UART history of
FW-42) is exactly the kind of lossy streaming path where this matters: data arrives in compressed chunks with
its own history window, and a loss has to be attributable to a specific point in that stream for a host
decoding it to stay synchronized.

**Fix:** carry the loss marker inline in the same ordered stream as the data (a sequence number, an in-band
marker) rather than as a side channel that can race the data path; a host decoding the stream should never have
to guess where in the data a reported loss happened.

**Detection:** test — needs a host-side decode test that injects a loss at a known point and checks the
decoded stream reports it at the right place, not just that a loss was reported at all.
