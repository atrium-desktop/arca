# Configuration Reference

This document provides the complete specification of configuration keys stored
in `$XDG_CONFIG_HOME/arca/arca.conf`.

---

## File Location

Arca resolves its configuration file in the following order:

1. `$XDG_CONFIG_HOME/arca/arca.conf`
2. `~/.config/arca/arca.conf` (if `XDG_CONFIG_HOME` is unset)

The configuration directory is created automatically on the first configuration save.
Saves are written via a temporary atomic file replacement (`write` + `rename`) to
prevent truncation or file corruption during unexpected termination.

---

## Configuration Keys

| Key | Type | Default | Allowed Values | Description |
|-----|------|---------|----------------|-------------|
| `show_hidden` | Boolean | `false` | `true`, `false` | When `true`, dotfiles and hidden directories are shown in file listings. |
| `show_thumbnails` | Boolean | `true` | `true`, `false` | When `true`, off-thread thumbnail decoding is enabled for images and audio files. |
| `sort` | String | `name` | `name`, `size`, `modified` | The primary sort attribute used to order directory listings. |
| `sort_ascending` | Boolean | `true` | `true`, `false` | When `true`, items are sorted ascending (A-Z, smallest to largest, oldest to newest). |
| `theme` | String | `system` | `system`, `light`, `dark` | Visual theme mode. `system` follows the compositor live; `light` and `dark` force fixed schemes. |
| `view` | String | `list` | `grid`, `list`, `miller` | The default content presentation mode used for new windows and tabs. |
| `bookmark` | String | *(empty)* | Filesystem path | Pinned custom bookmark path. May appear multiple times. |

---

## Parsing Semantics

- **Whitespace**: Leading and trailing whitespace around keys and values is trimmed.
- **Comments**: Lines beginning with `#` or `;` are ignored as comments.
- **Empty Lines**: Blank lines are ignored.
- **Unknown Keys**: Unrecognized keys are ignored without causing parse failures.
- **Repeated Keys**: For single-value keys, the last occurrence wins. For `bookmark`,
  all occurrences are collected into an ordered list.
- **Malformed Values**: Invalid values fall back to their respective defaults.

---

## Example File

```ini
# Arca file manager configuration
show_hidden = false
show_thumbnails = true
sort = name
sort_ascending = true
theme = system
view = list

# Custom pinned bookmarks
bookmark = /home/username/projects
bookmark = /home/username/work/documents
```
