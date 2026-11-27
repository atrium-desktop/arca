# Portal Chooser Protocol Reference

This document specifies the private inter-process communication protocol used
when `xdg-desktop-portal-atrium` invokes Arca via `--chooser-prompt`
(Atrium Arca FileChooser Contract v1, ADR-0027).

---

## Protocol Overview

- **Transport**: Standard input (`stdin`) and standard output (`stdout`) pipes.
- **Serialization**: UTF-8 JSON (strict envelopes with `deny_unknown_fields`).
- **Contract Version**: `1` (`ARCA_CONTRACT_VERSION`).
- **Lifecycle**: One-shot per request. The calling portal backend spawns
  `arca --chooser-prompt`, writes a single JSON request line to `stdin`,
  closes `stdin`, and reads a single JSON response line from `stdout`. Arca
  exits immediately after writing the response.

---

## Request Format

The chooser input packet carries the contract version and the flat
`FileChooserRequest` object:

```json
{
  "version": 1,
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
  }
}
```

### Request Fields

| Field | Type | Description |
|-------|------|-------------|
| `version` | Integer | Protocol contract version; must equal `1`. Checked before field decoding. |
| `request.mode` | String | Operation mode: `open_file`, `open_directory`, `save_file`, or `save_files`. |
| `request.app_id` | String | Calling application's desktop entry ID. |
| `request.title` | String | User-visible dialog title bar text. |
| `request.accept_label` | String or `null` | Custom text for confirmation button (GTK mnemonics like `_Open` are stripped). |
| `request.modal` | Boolean | Whether dialog should act modally relative to parent window. |
| `request.parent_window`| String or `null` | Wayland surface handle of the parent caller window. |
| `request.multiple` | Boolean | Allow selecting multiple paths in `open_file` and `open_directory` modes. |
| `request.current_folder` | Byte array or `null` | Initial directory encoded as raw Unix bytes (`BytePath`). |
| `request.current_name` | String or `null` | Pre-filled filename suggestion in `save_file` mode. |
| `request.current_file` | Byte array or `null` | Pre-selected existing file path in raw Unix bytes. |
| `request.filters` | Array of Filter | Available file type filters (`glob` / `mime` rules). |
| `request.current_filter` | Filter or `null` | The initially selected filter. |
| `request.choices` | Array of Choice | Caller-requested checkboxes (`options: []`) or dropdown controls. An empty `selected` string (`""`) defaults to `"false"` for checkboxes or the first option ID for dropdowns. |
| `request.files` | Array of Byte array | Suggested basenames for `save_files` mode (must be non-empty for `save_files`, empty for all other modes). |

---

## Response Format

Arca writes exactly one JSON response line to `stdout`:

```json
{
  "version": 1,
  "response": <response_object>
}
```

### 1. Selected (`status: "selected"`)

Emitted when the user confirms their selection. For `save_files`, Arca resolves
the suggested basenames inside the chosen folder (avoiding existing files and
duplicate names) and returns the final `request.files.len()` absolute paths.
`choices` answers the offered controls in request order.

```json
{
  "version": 1,
  "response": {
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
}
```

### 2. Cancelled (`status: "cancelled"`)

Emitted when the user dismisses the dialog via `Esc`, window close, or Cancel:

```json
{
  "version": 1,
  "response": {
    "status": "cancelled"
  }
}
```

### 3. Failed (`status: "failed"`)

Emitted if request parsing/validation fails, the contract version is unsupported,
or an unrecoverable error occurs:

```json
{
  "version": 1,
  "response": {
    "status": "failed",
    "message": "unsupported chooser contract version 6 (this build speaks 1)"
  }
}
```
