<p align="center">
  <img src="assets/arca-logo.png" alt="Arca Logo" width="160" height="160">
</p>

# Arca

[English](README.md) | [简体中文](README-zh.md)

> The project name **Arca** derives from the Latin *arca* — historically a chest, coffer, box, or ark for safekeeping precious documents and treasures. In modern computing, Arca serves as a fast, secure, and keyboard-first vessel for navigating and managing files across the Wayland desktop.

Arca is a fast file manager for Wayland and a companion application for the Tessera desktop ecosystem. While its visual language aligns with the Tessera design system, it operates standalone on any standard Wayland compositor. It is built on the Optics graphics and UI stack (**optics-app** application shell · **optics-gfx** Vulkan renderer · **optics-ui** immediate-mode UI · **optics-window** platform runtime · **optics-motion** motion dynamics).

![Rust](https://img.shields.io/badge/rust-1.85%2B-informational)

---

## Features

- **Keyboard-First Navigation** — Full history (back/forward/up), editable location bar (`Ctrl+L`) with tab completion, one-click bookmarks (Home, XDG user dirs, custom user pins).
- **Three View Modalities** — Responsive icon grid, detailed sortable list, and cascading Miller columns; large folders stay fluid and memory-bounded thanks to strictly virtualized row allocation.
- **Off-Thread Thumbnails** — High-performance image and album art (FLAC/MP3) thumbnail extraction cached according to the FreeDesktop thumbnail standard; rendered via GPU textures.
- **Multi-Tab Workspace** — Independent location, history, selection, and filter states across tabs with drag-and-drop reordering (`Ctrl+T`, `Ctrl+W`, `Ctrl+Tab`, `Ctrl+1..9`).
- **Quick Look Inspection** — Press `Space` for an animated preview modal (text excerpts, image dimensions and artwork, embedded album art) with Optics `optics-motion` spring easing; dismiss with `Space`, `Esc`, or a click outside.
- **Safe File Operations** — Open with default application, new folder creation (`Ctrl+Shift+N`) with automatic collision resolution, safe inline rename (`Ctrl+E`/`F2`) that never clobbers, FreeDesktop-compliant trash/restore/purge (`Del`/`Shift+Del`), copy/cut/paste (`Ctrl+C/X/V`) honouring `text/uri-list` and `x-special/gnome-copied-files`, drag-and-drop within the app and to/from other applications, multi-step undo/redo, and context menus. Large transfers run on a background worker with progress in the status bar.
- **XDG Desktop Portal File Chooser** — Native portal file chooser prompter (`arca --chooser-prompt`) speaking contract v6, supporting `OpenFile`, `SaveFile`, `SaveFiles`, `OpenDirectory`, multi-select, glob/MIME filters, custom choice controls, and overwrite warnings; the response is validated against the request before it is emitted. Also usable as a standalone CLI picker (`--choose-file`, `--choose-files`, `--choose-dir`, `--save-file`).
- **Adaptive Theming** — Follows system light/dark preference live, with toolbar appearance settings supporting System / Light / Dark modes.
- **Persistent Settings** — View mode, hidden files flag, sort order, theme, and bookmarks remembered in `~/.config/arca/arca.conf`.

---

## Quick Start

### 1. Build

Arca links the Optics C libraries (`optics-app` / `optics-window` / `optics-ui` / `optics-gfx`) through their official Rust bindings. The workspace enables Optics's `static` Cargo feature by default so canonical and release builds embed Optics automatically; pass `OPTICS_DYNAMIC=1` (as local development worktrees do) to link the shared objects instead. The system Vulkan loader, Wayland client, and font stack always stay dynamic:

```bash
cargo build --release
```

### 2. Launch Interactive File Manager

```bash
# Launch Arca in current working directory
./target/release/arca

# Or open a specific path in Miller columns view
./target/release/arca --miller ~/Documents
```

### 3. Run as Standalone CLI File Picker

```bash
# Pick a single file (prints chosen path to stdout)
./target/release/arca --choose-file ~/Downloads

# Pick a directory
./target/release/arca --choose-dir ~/Projects

# Save a file with a proposed default filename
./target/release/arca --save-file ~/Documents/report.pdf
```

### 4. Run as XDG Desktop Portal Prompter

```bash
# Reads JSON request from stdin and emits JSON response to stdout
echo '{"title":"Select File","multiple":false,"directory":false}' | ./target/release/arca --chooser-prompt
```

---

## CLI Cheatsheet

| Command | Description |
| :--- | :--- |
| `arca [path]` | Open interactive file manager at `[path]` (or current working directory) |
| `arca --grid [path]` | Open interactive file manager in **Grid** view |
| `arca --list [path]` | Open interactive file manager in **List** view |
| `arca --miller [path]` | Open interactive file manager in **Miller columns** view |
| `arca --choose-file [path]` | Standalone CLI single-file picker (prints path to stdout) |
| `arca --choose-files [path]` | Standalone CLI multi-file picker (prints paths to stdout) |
| `arca --choose-dir [path]` | Standalone CLI directory picker (prints path to stdout) |
| `arca --save-file <path>` | Standalone CLI save-file picker (returns target path to stdout) |
| `arca --chooser-prompt` | Run as XDG Desktop Portal FileChooser prompter (JSON over stdio) |
| `arca --help` | Display command-line options and usage |
| `arca --version` | Display Arca version |

---

## Keyboard Shortcuts

| Keys | Action |
| :--- | :--- |
| `↑` / `↓` | Move selection up / down |
| `←` / `→` | Traverse across grid / navigate Miller columns |
| `Enter` | Open selected directory or execute file |
| `Space` | Toggle animated Quick Look preview |
| `Backspace` / `Alt+↑` | Navigate to parent directory |
| `Alt+←` / `Alt+→` | Step back / forward in browsing history |
| `Ctrl+L` | Focus editable location bar |
| `Ctrl+F` | Focus live filter input field |
| `Ctrl+R` | Refresh directory listing |
| `Ctrl+H` | Toggle visibility of hidden dotfiles |
| `Ctrl+Shift+N` | Create new folder with inline rename |
| `Ctrl+E` | Rename selected file/folder inline |
| `Del` | Move selected items to FreeDesktop Trash |
| `Ctrl+C` / `Ctrl+X` / `Ctrl+V` | Copy / cut / paste selected files |
| `Ctrl+T` / `Ctrl+W` | Open / close browser tab |
| `Ctrl+Tab` / `Ctrl+Shift+Tab` | Switch to next / previous tab |
| `Ctrl+1` … `Ctrl+9` | Direct switch to tab by index |
| `Esc` | Close popup, cancel inline rename, or exit |

---

## Architecture

Arca is organized as a concentric Cargo workspace across four distinct layers:

| Crate | Layer | Role & Subsystem Boundaries |
| :--- | :--- | :--- |
| **`crates/arca`** | Entrypoint | Thin host binary: CLI argument parsing, standalone picker dispatch, Wayland event loop, portal prompter lifecycle. |
| **`crates/arca-ui`** | Presentation | View/presentation layer on Optics (`optics-app`/`optics-ui`/`optics-window`/`optics-gfx`/`optics-motion`): virtualized listing views, tabs, animated Quick Look, 1 MiB frame arena budget. |
| **`crates/arca-engine`** | Domain Engine | Domain business logic: directory model, navigation history, bookmarks, safe file ops, configuration, thumbnail cache. Headless & zero GUI dependencies. |
| **`crates/arca-xdg`** | Specifications | Pure standard library (`std` only): Base Dirs, User Dirs, RFC 8089 URIs, Shared MIME-info, Desktop Entries, `mimeapps.list`, Trash v1.0, RFC 1321 MD5. |

---

## Documentation

- **[Getting Started](docs/tutorials/01-getting-started.md)**: Guided walkthrough for launching Arca, navigating folders, and using tabs.
- **[How-To Guides](docs/how-to/index.md)**: Task recipes for file operations, customizing appearance, using Quick Look, and portal picker integration.
- **[CLI Reference](docs/reference/cli.md)**: Command-line options, flags, and picker modes.
- **[Configuration Reference](docs/reference/configuration.md)**: Syntax, default values, and keys in `~/.config/arca/arca.conf`.
- **[Keyboard Shortcuts Reference](docs/reference/keyboard-shortcuts.md)**: Complete list of keyboard controls and accelerators.
- **[Portal Prompter Protocol Reference](docs/reference/portal-protocol.md)**: Prompter process contract, JSON request/response schema.
- **[Architecture & Virtualized Rendering](docs/explanation/architecture-and-rendering.md)**: Immediate-mode UI, 1 MiB arena budget, and viewport virtualization.
- **[Subsystem Architecture Blueprint](docs/dev/subsystems.md)**: Living blueprint of workspace topology, crate boundaries, and invariant matrix.
- **[Engineering Standards](docs/dev/standards/README.md)**: Authoritative standards suite for architecture, invariants, ADRs, and workflows.
- **[Architectural Decision Records (ADR)](docs/adr/index.md)**: Immutable technical decisions, trade-offs, and negative knowledge.
