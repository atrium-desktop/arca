# Lantern

A fast file manager for Wayland, built on the
[optics](../optics) graphics/UI stack (**flux** Vulkan renderer · **lens**
immediate-mode UI · **iris** application toolkit).

![Rust](https://img.shields.io/badge/rust-1.85%2B-informational)

## Features

- **Navigation** — back/forward/up history, editable location bar (`Ctrl+L`),
  one-click bookmarks (Home, XDG user dirs, your own pins)
- **Three views** — an icon grid, a detailed sortable list, and responsive
  Miller columns that keep the active path in view
- **Tabs** — independent location, history, selection, and filter state in
  every tab (`Ctrl+T`, `Ctrl+W`, `Ctrl+Tab`)
- **Quick Look** — press `Space` for an animated preview with bounded text,
  directory, file, and image-metadata inspection
- **Listing tools** — file-type icons, sort by name / size / modified, live
  filter (`Ctrl+F`), hidden-file toggle (`Ctrl+H`)
- **File operations** — open with default app (double-click / `Enter`),
  new folder (`Ctrl+Shift+N`), rename (`Ctrl+E`), move to trash (`Del`,
  freedesktop-compliant, restorable), copy / cut / paste (`Ctrl+C/X/V`),
  right-click context menu
- **Theme** — follows the system light/dark preference live; the sidebar
  cycles System → Light → Dark
- **Persistent** — view mode, hidden-files flag, sort order, theme and bookmarks
  are remembered in `~/.config/lantern/lantern.conf`

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

Lantern links against the optics C libraries (iris / lens / flux) via their
official Rust bindings, resolved from the tagged optics monorepo. Install
the matching optics release once so `pkg-config` can find it:

```bash
cd ../optics
meson setup build
meson compile -C build
sudo meson install -C build
```

Then build Lantern:

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
# or
./target/release/lantern
```

Requires a Wayland session and a Vulkan-capable GPU.

## Configuration

`$XDG_CONFIG_HOME/lantern/lantern.conf` (usually
`~/.config/lantern/lantern.conf`) — a small `key = value` file:

```ini
show_hidden = false
sort = name
sort_ascending = true
theme = system          # system | light | dark
view = list             # grid | list | miller
bookmark = /home/you/projects
```

## Architecture

Lantern is a Cargo workspace with three crates:

| Crate | Role |
|-------|------|
| `crates/lantern-core` | Pure business logic — tab sessions, directory/Miller models, previews, history, bookmarks, sorting/filtering, file operations, trash, config. No GUI deps; fully unit-tested. |
| `crates/lantern-ui` | View/interaction layer on `lens`/`iris` — responsive grid/list/Miller rendering, animated Quick Look, tabs and chrome. Headless tests drive real frames. |
| `crates/lantern` | Thin binary: builds `AppState`, opens the `iris` window, runs the loop. |

See [docs/dev/optics-migration.md](docs/dev/optics-migration.md) for the
migration record from the legacy flux/flux-ui stack.
