# How to Use the Portal File Picker

This guide explains how to invoke Arca as a standalone command-line file chooser
and how to integrate it as an XDG Desktop Portal file picker prompter on Wayland.

---

## Use Standalone CLI File Chooser

Arca can be invoked from shell scripts, terminal workflows, or automated tools to
prompt the user for file and directory selection.

### Select a Single File (`--choose-file`)

To prompt the user to choose an existing file:

```bash
SELECTED=$(arca --choose-file --title "Select Document" ~/Documents)
echo "Selected: $SELECTED"
```

- When the user selects a file and clicks **Open** (or presses `Enter`), Arca writes
  the chosen absolute path to standard output and exits with code `0`.
- If the user cancels or presses `Esc`, Arca exits with code `1` and emits no path.

### Select Multiple Files (`--choose-files`)

To allow multiple file selection:

```bash
arca --choose-files --title "Select Attachments" ~/Pictures
```

- Each selected file path is written to standard output on a separate line.

### Select a Directory (`--choose-dir`)

To prompt the user to choose a folder:

```bash
DEST=$(arca --choose-dir --title "Choose Destination" ~)
```

- Only folders can be selected; files are shown dimmed or non-selectable.

### Save a File (`--save-file`)

To prompt the user for a destination save path:

```bash
TARGET=$(arca --save-file --name "report.pdf" --title "Export PDF" ~/Documents)
```

- The save dialog displays a filename input prefilled with the proposed name
  (`report.pdf`).
- If the selected filename already exists in the destination folder, Arca displays
  an overwrite warning confirmation modal.

---

## Integrate with XDG Desktop Portal

Arca complies with `org.freedesktop.impl.portal.FileChooser` version 3 and the Atrium
Arca FileChooser contract v1 (see [Portal Chooser Protocol Reference](../reference/portal-protocol.md)).

### Run in Chooser Prompt Mode

When invoked by `xdg-desktop-portal-atrium`, Arca runs with:

```bash
arca --chooser-prompt
```

The portal backend writes one versioned JSON request envelope to standard input:
- Operation mode (`open_file`, `open_directory`, `save_file`, `save_files`)
- Filter lists (glob patterns such as `*.png`, MIME types such as `image/*`)
- Custom choices (checkboxes and dropdowns declared by portal clients)
- Proposed directory, filename, or `save_files` suggested basenames

Arca responds on standard output with a versioned JSON envelope containing raw
Unix byte paths (`BytePath`):

```json
{
  "version": 1,
  "response": {
    "status": "selected",
    "paths": [[47, 116, 109, 112, 47, 110, 111, 116, 101, 115, 46, 116, 120, 116]],
    "current_filter": null,
    "choices": []
  }
}
```

If cancelled:

```json
{
  "version": 1,
  "response": {
    "status": "cancelled"
  }
}
```

### Desktop Environment Configuration

To configure your Wayland desktop session to route `FileChooser` requests
through `xdg-desktop-portal-atrium` (which spawns `arca --chooser-prompt`),
add or edit `/etc/xdg/xdg-desktop-portal/tessera-portals.conf` (or user-level
`~/.config/xdg-desktop-portal/portals.conf`):

```ini
[preferred]
default=atrium
org.freedesktop.impl.portal.FileChooser=atrium
```
