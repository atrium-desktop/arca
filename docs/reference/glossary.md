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
- **optics-gfx**: Low-overhead Vulkan renderer and scene graph.
- **optics-ui**: Immediate-mode UI layout engine and widget toolkit.
- **optics-window**: Native windowing runtime, Wayland event loop, and
  device management.
- **optics-app**: Application shell binding an `optics-ui` interface to an
  `optics-window` window.

Arca's release artifact embeds these modules in the `arca` binary (see
[ADR-0014](../adr/0014-optics-link-mode-and-static-release.md) and
[ADR-0015](../adr/0015-synchronize-with-optics-v0.0.83-and-shared-preferred-linking.md)).
The link mode is a build input: `OPTICS_STATIC=1`, `OPTICS_DYNAMIC=1`, the
`static` Cargo feature, or the upstream shared-preferred default.

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
guarantees that directories with 10,000+ files never exceed the `optics-ui` 1 MiB
per-frame memory arena budget.
