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

### Single-instance behaviour

Without `--new-window`, `arca [DIRECTORY]` forwards the path to the running
instance over its `$XDG_RUNTIME_DIR/arca-ipc.sock` socket and exits; the running
window opens it in a new tab and raises itself. `--new-window` skips that
hand-off and always creates a second process.

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
| `--title` | `<TITLE>` | Set the window and dialog title. |
| `--name` | `<NAME>` | Suggest an initial filename (valid only with `--save-file`). |

---

## Portal Prompter Option

| Option | Description |
|--------|-------------|
| `--chooser-prompt`, `--prompter` | Run in headless prompter mode, reading a JSON FileChooser request from standard input and writing the result JSON to standard output. `--prompter` is an alias. |

---

## Exit Codes

| Exit Code | Meaning |
|-----------|---------|
| `0` | Success; user confirmed selection or operation completed normally. |
| `1` | User cancelled dialog (`Esc` or Cancel button), or a fatal error occurred. |

---

## Environment Variables

| Variable | Description |
|----------|-------------|
| `XDG_CONFIG_HOME` | Directory for Arca configuration files (defaults to `~/.config`). |
| `XDG_CACHE_HOME` | Directory for decoded thumbnail cache (defaults to `~/.cache`). |
| `XDG_DATA_HOME` | Directory for trash and desktop data (defaults to `~/.local/share`). |
| `WAYLAND_DISPLAY` | Wayland compositor socket name. |
