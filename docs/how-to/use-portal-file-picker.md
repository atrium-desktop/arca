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

Arca complies with `org.freedesktop.impl.portal.FileChooser` version 3 and the Tessera
prompter protocol contract v6.

### Run in Prompter Mode

When invoked by a portal daemon (e.g., `xdg-desktop-portal-tessera`), Arca runs with:

```bash
arca --chooser-prompt
```

The daemon provides a JSON payload on standard input specifying:
- Operation mode (`open_file`, `open_directory`, `save_file`)
- Filter lists (glob patterns such as `*.png`, MIME types such as `image/*`)
- Custom choices (checkboxes and dropdowns declared by portal clients)
- Proposed directory and current filename

Arca responds on standard output with a JSON object:

```json
{
  "status": "success",
  "uris": ["file:///home/user/Documents/notes.txt"],
  "choices": []
}
```

If cancelled:

```json
{
  "status": "cancelled"
}
```

### Desktop Environment Configuration

To configure your Wayland desktop session to use Arca for all portal file chooser
requests, add or edit `/etc/xdg/xdg-desktop-portal/portals.conf` (or user-level
`~/.config/xdg-desktop-portal/portals.conf`):

```ini
[preferred]
default=tessera
org.freedesktop.impl.portal.FileChooser=arca
```
