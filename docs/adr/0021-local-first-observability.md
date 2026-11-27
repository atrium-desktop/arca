---
id: ADR-0021
title: "Local-First Observability: Logs, APM Spans, Watchdog Alerts, and Diagnostics"
status: accepted
date: 2026-11-25
scope: [engine/telemetry, presentation/ui, cli, testing]
superseded_by: null
negative_knowledge: false
---

# 0021. Local-First Observability: Logs, APM Spans, Watchdog Alerts, and Diagnostics

- Status: Accepted
- Date: 2026-11-25
- Deciders: Arca Architecture Working Group
- References: [Observability Reference](../reference/observability.md),
  [Interaction Latency](../dev/interaction-latency.md),
  [ADR-0020](0020-asynchronous-directory-navigation.md)

---

## Context and Problem Statement

"The window freezes after clicking around for a bit" could not be diagnosed
from a user report. Arca had no logging facade (a handful of ad-hoc
`eprintln!` calls), no latency measurements, and no record of what the UI
thread was doing when it stopped responding. The only working method was
attaching a debugger to a live process, which itself perturbs the compositor
connection being investigated.

Arca is a desktop application. Server-style observability (collectors, time
series databases, alert managers) would add daemons and network surfaces a
file manager must not have.

## Decision Drivers

- A freeze must leave evidence on disk that names the blocking operation.
- No network, no daemon, no new runtime dependency beyond the `log` facade.
- `arca-xdg` and `test-support` stay dependency-free (INV-ARCH-01).
- The prompter's stdout is a protocol channel and must never carry logs.
- Every signal is testable headlessly and deterministically.

## Considered Options

1. **Local-first, in-tree** — `log` facade plus an in-tree logger, metrics,
   spans, watchdog, and diagnostics snapshot in `arca_engine::telemetry`.
2. **Local-first plus OTLP export** — option 1 behind an opt-in feature that
   exports to an OpenTelemetry collector.
3. **`tracing` + `tracing-subscriber` + `env_logger`** — ecosystem crates.

## Decision Outcome

Chosen option: **1, local-first, in-tree.**

- **Log management.** Every crate above `arca-xdg` logs through `log`. The
  binary installs `telemetry::logging::Logger`: a size-rotated file at
  `$XDG_STATE_HOME/arca/logs/arca.log` (2 MiB × 5 generations) filtered by
  `ARCA_LOG`, and a stderr echo filtered by `ARCA_LOG_STDERR` (default
  `warn`). Status-bar messages are logged under `arca::status`, so a log
  reconstructs what the user saw.
- **APM.** `telemetry::span("name")` times a scope into a fixed-bucket
  latency histogram (p50/p95/p99/max). Spans opened on the UI thread publish
  the *current operation* and are held to a 100 ms budget.
- **Monitoring and alerting.** Each UI thread owns a frame heartbeat. A
  watchdog thread samples it every 100 ms and raises `ui_stall` (frame
  running > 750 ms, naming the open span; or, on Linux, the UI thread
  pinned at ≥ 90 % CPU for that long with no frame completing — a toolkit
  loop after Arca's build returned, which the frame heartbeat alone cannot
  see), `ui_stall_recovered`,
  `frame_storm` (≥ 400 frames/s for 3 s — the render loop is spinning, the
  signature of a lost compositor connection), and `error_burst`. Alerts are
  logged, kept in a ring buffer, deduplicated for 30 s, and shown in the
  window's status bar.
- **Diagnostics.** The window atomically rewrites
  `$XDG_STATE_HOME/arca/diagnostics-window.json` every 10 s and on exit;
  `arca --diagnostics` renders it, even while the window is frozen.
- **Stall stack capture.** On `ui_stall` (debug builds by default,
  `ARCA_STALL_CAPTURE=1` in release), Arca spawns `gdb`/`eu-stack` against
  itself, grants that one helper ptrace with `prctl(PR_SET_PTRACER, pid)` —
  which works under Yama `ptrace_scope=1`, where an outside debugger without
  root cannot attach — and writes every thread's stack to
  `$XDG_STATE_HOME/arca/stalls/`. One at a time, one per minute, 60 s
  timeout, newest 20 kept, skipped when already traced.

### Positive Consequences

- A freeze report now has a log line such as
  `[ui_stall] UI unresponsive for 2140 ms in fs.read_dir` and a latency table.
- A freeze also leaves the stuck stack on disk. The first freeze after this
  ADR (an Optics store loop after Arca's build returned) was invisible to the
  frame heartbeat and could only be diagnosed by a root `gdb` attach; both
  gaps are now closed: the CPU-based `ui_stall` rule sees it, and the capture
  records it.
- `[INV-UI-04]` (no blocking directory read on the UI thread during
  interactive navigation) is mechanically tested through a per-thread
  counter instead of reviewed by eye.
- One new dependency (`log`), already in the lockfile transitively.

### Negative Consequences

- Telemetry costs a mutex and a map lookup per metric write. Spans sit on
  operation boundaries, not inner loops; the frame heartbeat is two atomics.
- Stall capture briefly stops the process (one `gdb` attach, typically under
  a second) and the narrow ptrace grant is a deliberate, revoked-after-use
  relaxation of Yama for one child pid. Off by default in release builds.
- No remote aggregation. Option 2 stays open: the snapshot schema is
  versioned (`schema: 1`) and an exporter could read the same registry.

## Pros and Cons of the Options

### Option 2 — OTLP export

- Good: fleet-wide dashboards.
- Bad: pulls an async runtime and gRPC/HTTP stack into a file manager for a
  user base that does not run collectors. Deferred, not rejected.

### Option 3 — `tracing` ecosystem

- Good: structured fields, mature subscribers.
- Bad: three crates and a span model the codebase does not need yet; the
  rotating file sink still had to be written. Tessera made the same call
  (Tessera ADR-0007): `log` now, migrate mechanically if structured spans
  ever earn their weight.
