# Observability Reference

Arca records logs, latency metrics, and health alerts locally. Nothing is sent
over the network. Design rationale: [ADR-0021](../adr/0021-local-first-observability.md).

---

## Files

| Path | Written by | Contents |
|------|------------|----------|
| `$XDG_STATE_HOME/arca/logs/arca.log` | window, chooser, prompter | Live log. Rotated at 2 MiB into `arca.log.1` … `arca.log.4`. |
| `$XDG_STATE_HOME/arca/diagnostics-window.json` | window | Snapshot of metrics and recent alerts, rewritten every 10 s and on exit. |
| `$XDG_STATE_HOME/arca/stalls/stall-<UTC>-<role>-<pid>.txt` | any role, on `ui_stall` | Every thread's backtrace, taken while the process was stuck (see [Stall Stack Capture](#stall-stack-capture)). Newest 20 kept. |

`$XDG_STATE_HOME` defaults to `~/.local/state`. One-shot commands (`--help`,
`--version`, `--diagnostics`, forwarding to a running instance) write nothing.

---

## Environment Variables

| Variable | Default | Meaning |
|----------|---------|---------|
| `ARCA_LOG` | `info` | Filter for the log file. |
| `ARCA_LOG_STDERR` | `warn` | Filter for the stderr echo. `off` silences it. |
| `ARCA_STALL_MS` | `750` | A UI frame running longer than this raises `ui_stall`. |
| `ARCA_STALL_CAPTURE` | on in debug builds, off in release | `1`/`on`/`true`/`yes` or `0`/`off`/`false`/`no`: save all thread stacks on `ui_stall`. Anything else keeps the build default. |

A filter is a default level plus optional `target=level` overrides,
comma-separated; the longest matching target prefix wins:

```text
ARCA_LOG=debug                                # everything at debug
ARCA_LOG=info,arca_engine::watch=trace        # trace the inotify watcher only
ARCA_LOG=warn,arca::status=info               # quiet, but keep status messages
```

Levels: `off`, `error`, `warn`, `info`, `debug`, `trace`.

---

## Log Format

One record per line; embedded newlines are indented by four spaces.

```text
2026-11-25T01:02:03.456Z WARN  [main] arca::alert: [ui_stall] UI unresponsive for 2140 ms in fs.read_dir
```

`TIMESTAMP` (UTC, RFC 3339, ms) · `LEVEL` · `[thread]` · `target` · message.

| Target | Records |
|--------|---------|
| `arca` | Process start (`role=`, `pid=`, log path), unknown options. |
| `arca::capture` | Stall stack captures: the file written, or why none was. |
| `arca::status` | Every status-bar message, exactly as shown. |
| `arca::alert` | Raised alerts (see below). Level follows severity. |
| `arca::apm` | Over-budget spans. |
| `arca_engine::nav` | Navigation requests and outcomes (`debug`). |
| `arca_engine::worker` | Copy/move jobs: size, duration, failures. |
| `arca_engine::watch` | inotify overflow (`warn`), unwatchable paths (`debug`). |
| `arca_engine::thumbs` | Decoder panics. |
| `arca_engine::ops` | Launched files. |
| `arca_ui` | Window start/stop, socket availability, run errors. |

stdout is never written by the logger: in `--prompter` mode it carries the
portal protocol.

---

## Latency Histograms (APM spans)

Each span is timed into the histogram of the same name. Buckets: 0.1, 0.25,
0.5, 1, 2.5, 5, 10, 16.7, 33.3, 50, 100, 250, 500, 1000, 2500, 10000 ms, plus
overflow. Quantiles report the bucket bound, capped at the observed maximum.

| Histogram | Thread | Measures |
|-----------|--------|----------|
| `ui.frame` | UI | One full frame build. |
| `ui.stall` | watchdog | Duration of each stall that recovered. |
| `nav.latency` | UI | Click to published listing (asynchronous navigation). |
| `fs.read_dir` | any | One directory read, stat, and sort. |
| `state.refresh` | UI | Re-read of the current directory. |
| `state.miller_rebuild` | UI | Miller column model rebuild (Miller view only). |
| `miller.preview_read` | worker | Directory read for the Miller preview column. |
| `state.trash_index` | UI | Trash metadata index rebuild. |
| `preview.load` | UI | Quick Look data load. |
| `ui.activation_token` | UI | XDG activation token request (up to ~1 s). |
| `xdg.applications_for_mime` | UI | First "Open with" lookup per MIME type. |
| `ui.thumb_upload` | UI | One thumbnail texture upload. |
| `thumb.decode` | worker | One thumbnail decode (cache read-through). |
| `io.job` | worker | One copy/move job. |
| `launch.open` | UI | Launching a file with its default application. |

A span on the UI thread over **100 ms** logs under `arca::apm` and raises
`slow_operation`.

## Counters and Gauges

| Name | Meaning |
|------|---------|
| `nav.started` / `nav.completed` / `nav.failed` / `nav.superseded` | Asynchronous navigation outcomes. |
| `ui.blocking_io` | Directory reads performed on the UI thread. |
| `ui.slow_operations` | UI spans over budget. |
| `fs.watch.events` / `fs.watch.overflows` | Watcher-driven refreshes; inotify queue overflows. |
| `io.jobs_completed` / `io.jobs_failed` / `io.bytes` | Transfer outcomes and volume. |
| `thumb.decoded` / `thumb.unavailable` / `thumb.panics` | Thumbnail decode outcomes. |
| `ui.thumb_upload_deferred` | Decoded thumbnails left for a later frame because the per-frame upload budget (4 ms) was spent. |
| `log.errors` / `log.warnings` / `log.write_failures` | Logger activity. |
| `alerts.<kind>` / `alerts.suppressed` | Raised and deduplicated alerts. |
| `ui.fps` (gauge) | Frame rate over the storm window. |

---

## Alerts

| Kind | Severity | Fires when |
|------|----------|------------|
| `ui_stall` | critical | One frame has run longer than `ARCA_STALL_MS`; names the innermost open span. Also raised ("outside the frame build") when the UI thread holds ≥ 90 % of a core for `ARCA_STALL_MS` without completing a frame — a loop in the toolkit's layout/render after Arca's build returned (Linux; read from `/proc/thread-self/schedstat`). |
| `ui_stall_recovered` | warning | That frame completed; reports the longest observed duration. |
| `frame_storm` | critical | ≥ 400 frames/s sustained for 3 s: the render loop is spinning with nothing reaching the screen (for example, a lost compositor connection). |
| `error_burst` | warning | ≥ 20 errors logged within 60 s. |
| `slow_operation` | warning | One UI-thread span exceeded 100 ms. |

Each alert is logged, counted, kept (last 64) in the diagnostics snapshot,
and, at warning or above, shown once in the window's status bar prefixed
with `⚠`. Repeats with the same summary (ignoring numbers) are suppressed for
30 s. An idle window never stalls: between frames the heartbeat reads idle.

---

## Stall Stack Capture

When `ui_stall` fires and capture is enabled (`ARCA_STALL_CAPTURE`), Arca
saves every thread's backtrace while the process is still stuck, so a freeze
leaves the exact looping function on disk without anyone attaching a
debugger. Linux; needs `gdb` (preferred) or elfutils `eu-stack` on `PATH`.

How it works under Yama `ptrace_scope=1` (most distributions), where only a
process's ancestors may trace it:

1. Arca spawns `sh -c 'read _ && exec gdb -p <arca-pid> -batch …'`; the
   helper waits on stdin, so it cannot attach early.
2. `prctl(PR_SET_PTRACER, <helper-pid>)` allows exactly that pid to attach.
3. Arca releases the helper; `gdb` (same pid) attaches, prints
   `info threads` and `thread apply all bt 64`, and detaches.
4. The grant is revoked, the helper is killed if it runs past 60 s, and
   old captures beyond 20 are deleted. The path is logged under
   `arca::capture`.

`gdb` runs with `-nx` (no user init files), auto-load off, and debuginfod
disabled, so a capture never runs user scripts or waits on the network.
At most one capture runs at a time and at most one per minute. Arca skips
the capture when it is already being traced (your own debugger owns the
process). With `ptrace_scope` 2 or 3 the kernel refuses all non-root
attaches; the capture file then records the helper's error.

Each file starts with a header (version, time, role, pid, the alert summary,
tool, `ptrace_scope`, executable) followed by the tool's output. Attach it
to a bug report. In the dump, find the thread whose name is the process
name (`arca`, thread 1): its innermost frames are the stuck code.

---

## `arca --diagnostics`

Prints the window's latest snapshot: version, pid (running or exited),
uptime, log path, a latency table, counters, and the 20 most recent alerts,
followed by the five newest stall captures, if any. It reads files only,
so it works while the window is frozen.

| Exit code | Meaning |
|-----------|---------|
| `0` | Snapshot printed. |
| `1` | No snapshot yet, or the file is unreadable or corrupt. |

### Snapshot schema (`schema: 1`)

```json
{
  "schema": 1, "version": "0.2.17", "role": "window", "pid": 4242,
  "started_unix_ms": 0, "written_unix_ms": 0,
  "log_file": "/home/u/.local/state/arca/logs/arca.log",
  "metrics": {
    "counters":   { "nav.started": 12 },
    "gauges":     { "ui.fps": 60 },
    "histograms": { "ui.frame": { "count": 900, "sum_ms": 0.0, "mean_ms": 0.0,
                                  "p50_ms": 0.0, "p95_ms": 0.0, "p99_ms": 0.0, "max_ms": 0.0 } }
  },
  "alerts": [ { "kind": "UiStall", "severity": "Critical",
                "summary": "UI unresponsive for 900 ms in fs.read_dir", "at_unix_ms": 0 } ]
}
```
