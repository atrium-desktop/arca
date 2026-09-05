# Glossary

This document defines canonical terms used throughout Arca's codebase,
architecture, and documentation.

---

## Terms

### BytePath
A raw byte representation (`Vec<u8>`) of a filesystem path on POSIX platforms,
preserving non-UTF-8 filenames without loss or panic.

### Miller Columns
A navigation presentation style where cascading, side-by-side vertical columns
represent hierarchy levels. Expanding a directory creates a new child column to
its right while maintaining the ancestor hierarchy in view.

### Optics Stack
The native graphics and UI monorepo powering Arca and Tessera:
- **Flux**: Low-overhead Vulkan renderer and scene graph.
- **Lens**: Immediate-mode UI layout engine and widget toolkit.
- **Iris**: Application windowing runtime, Wayland event loop, and device management.

### Portal Prompter
A standalone user-interface process executed by a desktop portal daemon to handle
file or credential selection on behalf of sandboxed client applications.

### Quick Look
An on-demand modal overlay (`Space`) that inspects and displays previews of selected
files (metadata, thumbnails, audio tags, line previews) without launching external
applications.

### Virtualized Viewport
A rendering technique in `arca-ui` where only file rows physically visible within
the current viewport scroll bounds are generated as immediate-mode UI widgets. This
guarantees that directories with 10,000+ files never exceed Lens's 1 MiB per-frame
memory arena budget.
