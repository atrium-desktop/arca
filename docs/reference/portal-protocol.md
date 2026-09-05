# Portal Prompter Protocol Reference

This document specifies the private inter-process communication protocol used
when invoking Arca via `--chooser-prompt` (Tessera Prompter Process Contract v6).

---

## Protocol Overview

- **Transport**: Standard input (`stdin`) and standard output (`stdout`) pipes.
- **Serialization**: UTF-8 JSON.
- **Contract Version**: `6`.
- **Lifecycle**: One-shot per prompt. The calling daemon spawns Arca, writes a single
  request packet to `stdin`, closes or leaves `stdin` open, and reads a single response
  packet from `stdout`. Arca exits immediately after writing the response.

---

## Request Format

The prompter input packet contains a version stamp, the portal request parameters,
and an optional appearance snapshot.

```json
{
  "contract_version": 6,
  "request": {
    "mode": "open_file",
    "app_id": "org.mozilla.firefox",
    "title": "Open Image",
    "accept_label": "Choose",
    "modal": true,
    "parent_window": "wayland:01234567",
    "multiple": false,
    "current_folder": [47, 104, 111, 109, 101, 47, 117, 115, 101, 114],
    "current_name": null,
    "current_file": null,
    "filters": [
      {
        "label": "Images (*.png, *.jpg)",
        "rules": [
          {"kind": "glob", "value": "*.png"},
          {"kind": "glob", "value": "*.jpg"},
          {"kind": "mime", "value": "image/*"}
        ]
      }
    ],
    "current_filter": null,
    "choices": [
      {
        "id": "read_only",
        "label": "Open as read-only",
        "options": [],
        "selected": "false"
      }
    ],
    "files": []
  },
  "appearance": {
    "color_scheme": "system",
    "accent_color": {"red": 64, "green": 128, "blue": 255},
    "high_contrast": false,
    "reduced_motion": false
  }
}
```

### Request Fields

| Field | Type | Description |
|-------|------|-------------|
| `contract_version` | Integer | Protocol contract version; must match `6`. |
| `request.mode` | String | Operation mode: `open_file`, `open_directory`, `save_file`, or `save_files`. |
| `request.app_id` | String | Calling application's desktop entry ID. |
| `request.title` | String | User-visible dialog title bar text. |
| `request.accept_label` | String or `null` | Custom text for confirmation button. |
| `request.modal` | Boolean | Whether dialog should act modally relative to parent window. |
| `request.parent_window`| String or `null` | Wayland surface handle of the parent caller window. |
| `request.multiple` | Boolean | Allow selecting multiple files (only in `open_file` mode). |
| `request.current_folder` | Byte array or `null` | Initial directory encoded as raw Unix bytes. |
| `request.current_name` | String or `null` | Pre-filled filename suggestion in `save_file` mode. |
| `request.current_file` | Byte array or `null` | Pre-selected existing file path in raw Unix bytes. |
| `request.filters` | Array of Filter | Available file type filters. |
| `request.current_filter` | Filter or `null` | The initially selected filter. |
| `request.choices` | Array of Choice | Caller-requested checkboxes or dropdown controls. |
| `request.files` | Array of Byte array | Suggested basenames for `save_files` mode. |

---

## Response Format

Arca writes exactly one of three tagged JSON responses to `stdout`.

### 1. Selected (`status: "selected"`)

Emitted when the user confirms their selection:

```json
{
  "status": "selected",
  "paths": [
    [47, 104, 111, 109, 101, 47, 117, 115, 101, 114, 47, 112, 104, 111, 116, 111, 46, 112, 110, 103]
  ],
  "current_filter": {
    "label": "Images (*.png, *.jpg)",
    "rules": [
      {"kind": "glob", "value": "*.png"},
      {"kind": "glob", "value": "*.jpg"}
    ]
  },
  "choices": [
    ["read_only", "true"]
  ]
}
```

### 2. Cancelled (`status: "cancelled"`)

Emitted when the user dismisses the dialog via `Esc`, window close, or Cancel:

```json
{
  "status": "cancelled"
}
```

### 3. Failed (`status: "failed"`)

Emitted if request validation fails or an unrecoverable I/O error occurs:

```json
{
  "status": "failed",
  "message": "Invalid request: suggested files are valid only for SaveFiles"
}
```
