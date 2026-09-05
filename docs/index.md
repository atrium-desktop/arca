# Arca Documentation

Arca is a fast file manager for Wayland, companion application for the
[Tessera](https://github.com/aegis-shell/tessera) desktop, and standalone file
dialog prompter. It is built on the
[Optics](https://github.com/aegis-shell/optics) graphics stack (**flux** Vulkan
renderer · **lens** immediate-mode UI · **iris** application runtime).

For project introduction, feature summary, and quick build steps, see the root
[README](../README.md).

---

## Documentation Sections

| Section | Purpose |
|---------|---------|
| [Tutorials](tutorials/01-getting-started.md) | Hands-on, step-by-step walkthroughs for learning Arca |
| [How-To Guides](how-to/index.md) | Task-oriented recipes for common file management operations |
| [Reference](reference/index.md) | Complete CLI flags, configuration keys, shortcuts, and protocols |
| [Explanation](explanation/index.md) | Architectural background, viewport virtualization, and pipeline design |
| [Architecture Decision Records](adr/index.md) | Historical, immutable technical decisions |
| [Repository Governance](governance/index.md) | Architectural invariants, code review gates, and documentation standard |
| [Contributor Docs](dev/index.md) | Internal developer setup, automated testing, acceptance, and packaging |

---

## Quick Navigation by Topic

### Daily Usage & Navigation
- First-time walkthrough: read [Getting Started Tutorial](tutorials/01-getting-started.md).
- Path jumping, bookmarks, and tabs: read [How to Navigate and Browse Directories](how-to/navigate-and-browse.md).
- All keyboard controls: read [Keyboard Shortcuts Reference](reference/keyboard-shortcuts.md).

### File Management & Trash
- Creating folders, inline rename, copy/paste, and restoring trash: read
  [How to Manage Files and Use Trash](how-to/file-operations-and-trash.md).
- Quick file inspection (`Space`): read [How to Use Quick Look Preview](how-to/use-quick-look-preview.md).

### Portal Chooser & Command-Line
- Scripting file selection or using as Wayland portal chooser: read
  [How to Use the Portal File Picker](how-to/use-portal-file-picker.md).
- CLI options and flags: read [Command-Line Interface Reference](reference/cli.md).
- IPC protocol details: read [Portal Prompter Protocol Reference](reference/portal-protocol.md).

### Configuration & Customization
- Theming, thumbnail toggles, and view modes: read
  [How to Customize Appearance and Settings](how-to/customize-appearance-and-settings.md).
- Config file syntax: read [Configuration Reference](reference/configuration.md).

### Packaging & Installation
- Installing or building from source: read [How to Install and Package Arca](how-to/install-and-package.md).
- Troubleshooting display or portal issues: read [How to Troubleshoot Common Issues](how-to/troubleshoot-common-issues.md).
- Distribution packager guide: read [Distribution Packaging](dev/packaging.md).

### Architecture & Internals
- Immediate-mode UI and 1 MiB arena memory budget: read
  [Architecture and Virtualized Rendering](explanation/architecture-and-rendering.md).
- Asynchronous thumbnail pipeline: read
  [Thumbnail Extraction and Caching Pipeline](explanation/thumbnail-pipeline.md).
- Prompter sandboxing model: read
  [Portal Chooser Architecture and Isolation](explanation/portal-chooser-design.md).
- Architectural records: browse [ADR Index](adr/index.md).
