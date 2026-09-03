# Arca

A fast file manager for Wayland. Arca is a companion application for the
[tessera](../tessera-dev) desktop — its visual language follows the
[tessera design system](../tessera-dev/docs/dev/design) — but it runs standalone
on any Wayland compositor. It is built on the
[optics](../optics) graphics/UI stack (**flux** Vulkan renderer · **lens**
immediate-mode UI · **iris** application toolkit).

![Rust](https://img.shields.io/badge/rust-1.85%2B-informational)

## Features

- **Navigation** — back/forward/up history, editable location bar (`Ctrl+L`),
  one-click bookmarks (Home, XDG user dirs, your own pins)
- **Three views** — an icon grid, a detailed sortable list, and responsive
  Miller columns that keep the active path in view; large folders stay
  fluid thanks to virtualized rows
- **Thumbnails** — album covers (FLAC/MP3) and images appear in the grid and
  in Quick Look, decoded off-thread and cached on disk; toggle in settings
- **Tabs** — independent location, history, selection, and filter state in
  every tab (`Ctrl+T`, `Ctrl+W`, `Ctrl+Tab`)
- **Quick Look** — press `Space` for an animated preview with bounded text,
  directory, file, and image-metadata inspection
- **Listing tools** — sort by name / size / modified, live filter
  (`Ctrl+F`), hidden-file toggle (`Ctrl+H`); icons come from Arca's own
  SVG set (`assets/icons/`), registered with lens at runtime
- **File operations** — open with default app (double-click / `Enter`),
  new folder (`Ctrl+Shift+N`), rename (`Ctrl+E`), move to trash (`Del`,
  freedesktop-compliant, restorable), copy / cut / paste (`Ctrl+C/X/V`),
  right-click context menu
- **Portal Chooser / File Picker** — native XDG desktop portal file chooser
  provider (`arca --chooser-prompt`) supporting `OpenFile`, `SaveFile`,
  `OpenDirectory`, typed glob/MIME filters, custom portal choices, and
  overwrite protection; also usable as a standalone CLI picker (`--choose-file`,
  `--choose-files`, `--choose-dir`, `--save-file`)
- **Theme** — follows the system light/dark preference live; the toolbar
  settings button opens a System / Light / Dark picker
- **Persistent** — view mode, hidden-files flag, sort order, theme and bookmarks
  are remembered in `~/.config/arca/arca.conf`

## Keyboard shortcuts

| Keys | Action |
|------|--------|
| `↑` / `↓` | Move selection |
| `←` / `→` | Move across a grid / navigate Miller columns |
| `Enter` | Open directory / file |
| `Space` | Open / close Quick Look |
| `Backspace` / `Alt+↑` | Go up |
| `Alt+←` / `Alt+→` | Back / forward |
| `Ctrl+L` | Focus location bar (type a path, `Enter` to go) |
| `Ctrl+F` | Focus filter field |
| `Ctrl+R` | Refresh |
| `Ctrl+H` | Show / hide hidden files |
| `Ctrl+Shift+N` | New folder (renames inline) |
| `Ctrl+E` | Rename selection |
| `Del` | Move selection to trash |
| `Ctrl+C` / `Ctrl+X` / `Ctrl+V` | Copy / cut / paste |
| `Ctrl+T` / `Ctrl+W` | Open / close tab |
| `Ctrl+Tab` / `Ctrl+Shift+Tab` | Next / previous tab |
| `Ctrl+1` … `Ctrl+9` | Switch to tab |
| `Esc` | Quit |

## Building

Arca links against the optics C libraries (iris / lens / flux) via their
official Rust bindings, resolved from the tagged optics monorepo. Install
the matching optics release once so `pkg-config` can find it:

```bash
cd ../optics
meson setup build
meson compile -C build
sudo meson install -C build
```

Then build Arca:

```bash
cargo build            # debug
cargo build --release  # optimized
cargo test             # unit + headless-UI tests
```

To develop against a live sibling `../optics` checkout instead (no install,
local `[patch]` resolution), use the linked development worktree described
in
[docs/dev/cross-repository-development.md](docs/dev/cross-repository-development.md).

## Running

```bash
cargo run
# open a directory in a chosen view
cargo run -- --miller ~/projects
# run as XDG desktop portal file chooser prompter (JSON over stdio)
cargo run -- --chooser-prompt
# run as standalone CLI file picker
cargo run -- --choose-file ~/Documents
# or
./target/release/arca
```

Requires a Wayland session and a Vulkan-capable GPU.

## Configuration

`$XDG_CONFIG_HOME/arca/arca.conf` (usually
`~/.config/arca/arca.conf`) — a small `key = value` file:

```ini
show_hidden = false
sort = name
sort_ascending = true
theme = system          # system | light | dark
view = list             # grid | list | miller
show_thumbnails = true
bookmark = /home/you/projects
```

## Architecture

Arca is a Cargo workspace with three crates:

| Crate | Role |
|-------|------|
| `crates/arca-core` | Pure business logic — tab sessions, directory/Miller models, previews, history, bookmarks, sorting/filtering, file operations, trash, config. No GUI deps; fully unit-tested. |
| `crates/arca-ui` | View/interaction layer on `lens`/`iris` — responsive grid/list/Miller rendering, animated Quick Look, tabs and chrome. Headless tests drive real frames. |
| `crates/arca` | Thin binary: builds `AppState`, opens the `iris` window, runs the loop. |

See [docs/dev/optics-migration.md](docs/dev/optics-migration.md) for the
migration record from the legacy flux/flux-ui stack.
