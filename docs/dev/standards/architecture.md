# System Architecture Governance

This document codifies the workspace topology, subsystem boundaries, and non-negotiable architectural red-lines of Arca.

---

## 1. Concentric Workspace Topology

Arca organizes its Cargo workspace into four concentric layers. Dependencies flow strictly from outer layers inward; backward or circular dependencies across layer boundaries are prohibited:

```text
┌──────────────────────────────────────────────────────────────┐
│  Layer 4: Host Binary (`crates/arca`)                        │
│  - Unified single executable (`arca`)                        │
│  - CLI argument parsing, standalone picker dispatch          │
│  - Wayland application lifecycle & XDG portal prompt loop    │
├──────────────────────────────────────────────────────────────┤
│  Layer 3: Presentation & Viewport (`crates/arca-ui`)         │
│  - Optics shell (`optics-app`) · UI (`optics-ui`)            │
│  - Platform runtime (`optics-window`)                        │
│  - Rendering (`optics-gfx`) · motion (`optics-motion`)       │
│  - Virtualized directory listing views (Grid, List, Miller)  │
│  - Animated Quick Look preview & borderless floating chrome  │
├──────────────────────────────────────────────────────────────┤
│  Layer 2: Core Domain Engine (`crates/arca-engine`)          │
│  - Directory traversal & filesystem cache models             │
│  - Session state machine, forward/backward history, tabs     │
│  - Safe file operations (rename, collision-free folder, undo)│
│  - FreeDesktop trash integration & off-thread thumbnail cache│
│  - Portal FileChooser contract and filter matching           │
├──────────────────────────────────────────────────────────────┤
│  Layer 1: Pure Specifications & Contracts (`crates/arca-xdg`)│
│  - Pure standard library implementation (std only)           │
│  - XDG Base Directory & XDG User Dirs specifications        │
│  - RFC 8089 File URIs & standard clipboard payloads          │
│  - Shared MIME-info (magic sniffing + glob matching)         │
│  - Desktop Entry & mimeapps.list resolution hierarchy        │
│  - FreeDesktop Trash v1.0 metadata & RFC 1321 MD5            │
└──────────────────────────────────────────────────────────────┘
```

---

## 2. Architectural Red-Lines

### 2.1 Engine-UI Anti-Inversion
- **Red-Line**: `arca-engine` must NEVER depend directly or indirectly on `arca-ui`, `optics-app`, `optics-ui`, `optics-window`, `optics-gfx`, or any graphical/Wayland windowing runtime.
- **Enforcement**: Domain business logic, directory models, sorting algorithms, and file operations must remain 100% headless-testable without requiring a Wayland compositor or GPU rendering context.

### 2.2 XDG Specification Purity
- **Red-Line**: `arca-xdg` must NEVER depend on any workspace member or external crates.io package.
- **Enforcement**: It is strictly implemented using Rust's standard library (`std`). It serves as a zero-dependency Freedesktop protocol layer that can be leveraged across any desktop component without dependency baggage.

### 2.3 UI Frame Arena Memory Budget
- **Red-Line**: Viewport rendering in `arca-ui` must strictly adhere to the `optics-ui` 1 MiB per-frame widget arena budget.
- **Enforcement**: All directory listing views (**Grid**, **List**, and **Miller columns**) must remain strictly virtualized. Only visible row slices calculate layout nodes and allocate draw calls. Unbounded iteration over large directory collections in rendering code is prohibited and guarded by automated arena tests (`tests::large_directory_does_not_overflow_frame_arena`).

### 2.4 Non-Destructive File Operations & Trash Fidelity
- **Red-Line**: Destructive file deletions must default to compliant FreeDesktop trash with companion `.trashinfo` metadata (RFC 3339 timestamps and percent-encoded paths). Inline renaming and new folder creation must never silently clobber existing destinations.
- **Enforcement**: Operations are guarded by unit tests verifying deduplication ("New Folder", "New Folder (1)") and anti-clobber invariants.

### 2.5 Single Public Binary Singleton
- **Red-Line**: `arca` is the sole public executable binary produced by the workspace.
- **Enforcement**: Interactive file browsing, standalone command-line file picking (`--choose-file`), and XDG desktop portal file chooser prompting (`--chooser-prompt`) are executed through this unified binary.

---

## 3. Crate Addition and Workspace Integrity

1. **Approved External Dependencies**: `arca-*` crates remain std-only with one deliberate exception: `image` (with minimal `png` and `jpeg` features) in `arca-engine` for decoding thumbnail image streams. Any additional external crates require architecture review and ADR ratification.
2. **Crate Inclusion Threshold**: New crates are admitted to `crates/*` only if their responsibilities cannot be modeled as internal modules of existing layers and their position in the concentric hierarchy is validated via an ADR.
