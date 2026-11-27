# Command-Line Interface Reference

This document provides a reference for all command-line arguments, options,
and exit codes supported by the `arca` executable.

---

## Synopsis

```text
arca [OPTIONS] [DIRECTORY]
arca --chooser-prompt
arca --choose-file [--title TITLE] [DIRECTORY]
arca --choose-files [--title TITLE] [DIRECTORY]
arca --choose-dir [--title TITLE] [DIRECTORY]
arca --save-file [--name NAME] [--title TITLE] [DIRECTORY]
```

---

## General Options

| Option | Description |
|--------|-------------|
| `-h`, `--help` | Print help information and exit. |
| `-V`, `--version` | Print Arca version information and exit. |
| `--new-window` | Force a new window instead of handing the path to an already running instance. |
| `--grid` | Launch directly in Grid view mode. |
| `--list` | Launch directly in List view mode. |
| `--miller` | Launch directly in Miller columns view mode. |
| `--diagnostics` | Print the running (or last) window's latency metrics, counters, and recent alerts, then exit. Exits `1` when no snapshot exists yet. See [Observability](observability.md). |

### Single-instance behaviour

Without `--new-window`, `arca [DIRECTORY]` forwards the path (and any
`XDG_ACTIVATION_TOKEN`) to the running instance over its private `0600`
`$XDG_RUNTIME_DIR/arca/ipc.sock` socket (length-prefixed `arca-ipc-v2` frames
with `SO_PEERCRED` verification) and exits; the running window opens it in a
new tab and raises itself. `--new-window` skips that hand-off and always
creates a second process. Use `--` to pass a directory name that begins with
`-`.

---

## Standalone File Chooser Options

These options run Arca as a modal file or directory picker, outputting selected
paths to standard output upon confirmation.

| Option | Arguments | Description |
|--------|-----------|-------------|
| `--choose-file` | `[DIRECTORY]` | Open a single-file selection dialog. |
| `--choose-files` | `[DIRECTORY]` | Open a multi-file selection dialog. |
| `--choose-dir` | `[DIRECTORY]` | Open a directory selection dialog. |
| `--save-file` | `[DIRECTORY]` | Open a file save dialog. |
| `-0`, `--null` | — | Terminate output paths with NUL (`\0`) bytes instead of newlines for safe `xargs -0` consumption of arbitrary Unix filenames. |
| `--title` | `<TITLE>` | Set the window and dialog title. |
| `--name` | `<NAME>` | Suggest an initial filename (valid only with `--save-file`). |

---

## Portal Prompter Option

| Option | Description |
|--------|-------------|
| `--chooser-prompt`, `--prompter` | Run in headless portal chooser mode (Atrium Contract v1), reading a JSON FileChooser request from standard input and writing the result JSON to standard output. `--prompter` is an alias. |

---

## Exit Codes

| Exit Code | Meaning |
|-----------|---------|
| `0` | Success; user confirmed selection or operation completed normally. |
| `1` | User cancelled dialog (`Esc` or Cancel button), or a runtime error occurred. |
| `2` | Invalid command-line usage (unknown option). |

---

## Environment Variables

| Variable | Description |
|----------|-------------|
| `XDG_CONFIG_HOME` | Directory for Arca configuration files (defaults to `~/.config`). |
| `XDG_CACHE_HOME` | Directory for decoded thumbnail cache (defaults to `~/.cache`). |
| `XDG_DATA_HOME` | Directory for trash and desktop data (defaults to `~/.local/share`). |
| `XDG_STATE_HOME` | Directory for logs and diagnostics, under `arca/` (defaults to `~/.local/state`). |
| `ARCA_LOG` | Log-file filter, e.g. `info` or `warn,arca_engine::watch=debug` (default `info`). |
| `ARCA_LOG_STDERR` | stderr echo filter (default `warn`; `off` silences it). |
| `ARCA_STALL_MS` | UI-stall alert threshold in milliseconds (default `750`). |
| `WAYLAND_DISPLAY` | Wayland compositor socket name. |
