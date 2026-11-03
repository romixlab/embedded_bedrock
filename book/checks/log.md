# LOG checks, explained

Detail page for the `LOG` category of the [check catalogue](../checks.md#-log---logging). Each section explains
one check: what it catches, why the firmware fails without it, a concrete example, how it is checked, and the fix.
The one-line entries in the catalogue stay the source of truth for ID, severity and wording; this page is the
commentary.

## `LOG-1` RTT that can block <a id="log-1"></a>

🛑 error · check: src mid · [catalogue entry](../checks.md#log-1)

**What it catches.** An RTT channel left at its default mode, where probe-rs attaching sets the channel to
blocking — so a log call waits for the host to drain the buffer, and blocks forever if nothing is attached or the
host stalls.

**Why it matters.** Logging is supposed to be a side channel, not a dependency; a blocking RTT channel turns
"nobody attached a probe" or "the host debugger hung" into the firmware itself hanging at its next log call,
which is a surprising way to lose a device that has nothing wrong with it otherwise.

**Example.** typec_dongle FW-27, wire_weaver's STD-26, and embedded_bedrock ROB-8 all converge on the same fix:
force the channel's mode explicitly rather than trust the default.

**How to check.** `src mid`: the RTT channel's mode must be explicitly set to `NoBlockSkip` (or an equivalent
non-blocking mode), not left at whatever `rtt-target`/`defmt-rtt` defaults to.

**Fix.** Configure the RTT channel as `NoBlockSkip`: drop whole frames when the buffer is full and count the
drops, rather than ever waiting for the host.

## `LOG-2` `DEFMT_RTT_BUFFER_SIZE` and `DEFMT_LOG` not set explicitly <a id="log-2"></a>

⚠️ warning · check: cfg easy · [catalogue entry](../checks.md#log-2)

**What it catches.** `.cargo/config.toml` relying on `defmt-rtt`'s default buffer size and `DEFMT_LOG`'s default
level instead of setting both explicitly.

**Why it matters.** The default buffer is small enough that a burst right at boot (every "I'm alive" log a
firmware tends to emit during init) can overrun it before the host even attaches; a real measurement found 773 of
1000 init-time log lines lost to exactly this.

**Example.** The template's own conventions call this out, and a tpm log entry (Oct 2026) has the 773/1000
measurement that motivated making both settings explicit rather than left at their defaults.

**How to check.** `cfg easy`: both `DEFMT_RTT_BUFFER_SIZE` and `DEFMT_LOG` must be set as explicit env vars in
`.cargo/config.toml`, not left unset.

**Fix.** Set `DEFMT_RTT_BUFFER_SIZE` large enough to absorb an init burst before the host attaches, and
`DEFMT_LOG` to the level the firmware actually wants shipped, both explicitly.

## `LOG-3` Hot-path logging at info or above <a id="log-3"></a>

💡 suggestion · check: review · [catalogue entry](../checks.md#log-3)

**What it catches.** A log call at `info` level or above on a path that runs often (per packet, per poll, per
loop iteration), rather than reserved for events that happen occasionally.

**Why it matters.** Even a non-blocking RTT channel (`LOG-1`) costs cycles to format and enqueue; a hot path
logging at a level that's typically left on in the field pays that cost continuously, and can itself become the
performance problem under investigation (`PERF-4`/`PERF-5`'s territory from the other direction).

**How to check.** Not automated (`review`): identify hot paths from the firmware's own structure and check their
log level against how often the path actually runs.

**Fix.** Move hot-path logging to `trace`/`debug` (off by default per `DEFMT_LOG`, `LOG-2`), and keep `info` and
above for events that are genuinely occasional.

## See also

- [Review checklist](../review-checklist.md) — *Host input* and *Build* condense `LOG-1` and `LOG-2`.
- [Bring-up checklist](../bringup-checklist.md) step 7 (*Load*) exercises `LOG-1` directly against real traffic.
- [`IRQ-7`](irq.md#irq-7) — the same "keep it short" logic applied to interrupt handlers rather than hot loops.
