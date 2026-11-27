# Subsystem Architecture: Arca System Blueprint

- Status: Living Blueprint
- Last Updated: 2026-11-20
- Scope: workspace/topology, architecture/subsystems, presentation/optics-ui, engine/state, protocols/xdg
- Maintainers: Arca Architecture Working Group

---

## 1. System Overview & Monorepo Topology

Arca is a fast, keyboard-first file manager for Wayland and a companion application for the Tessera desktop ecosystem. The project name derives from the Latin *arca* — historically a chest, coffer, or ark for safekeeping valuable documents and treasures. In modern computing, Arca serves as a secure, fast, and visually refined vessel for browsing and managing files across Wayland compositors.

It is organized as a modern Cargo workspace containing four production crates
arranged across concentric architectural layers, plus two test-only workspace
members (`crates/test-support` and `tests/`) that are never normal or build
dependencies of a production crate:

- **`crates/test-support`** — std-only fixtures shared by external suites:
  scratch directories with RAII cleanup, serialized XDG environment isolation,
  bounded polling, and built-binary lookup.
- **`tests/` (`arca-tests`)** — one root integration binary and one root
  end-to-end binary. The architecture firewalls, cross-crate flows, headless
  user journeys, prompter contract, IPC handshake, virtualization, and
  interaction-stress guards live here; see [Automated Testing](testing.md).

The production topology:

```text
┌────────────────────────────────────────────────────────────────────────┐
│                   Application Entry Point (Root)                       │
│   crates/arca (Unified Executable: CLI, Standalone Picker, Prompter)   │
└───────────────────┬────────────────────────────────┬───────────────────┘
                    │                                │
                    ▼                                ▼
┌────────────────────────────────────────────────────────────────────────┐
│               Presentation & Viewport Subsystem                        │
│   crates/arca-ui (Optics App shell, UI, Gfx GPU, Motion, Chrome)       │
└───────────────────┬────────────────────────────────────────────────────┘
                    │
                    ▼
┌────────────────────────────────────────────────────────────────────────┐
│               Application Domain Engine & State Machine                 │
│   crates/arca-engine (Directory Model, Navigation, History, Ops, Config)│
└───────────────────┬────────────────────────────────────────────────────┘
                    │
                    ▼
┌────────────────────────────────────────────────────────────────────────┐
│               Pure Freedesktop Specifications & Protocols              │
│   crates/arca-xdg (Zero-Dependency Base Dirs, User Dirs, MIME, Trash)  │
└────────────────────────────────────────────────────────────────────────┘
```

---

## 2. Component Breakdown & Boundaries

### 2.1 `crates/arca-xdg` (Pure Freedesktop Protocols)
- **Role**: Pure-Rust, zero-dependency implementation of standard Freedesktop.org specifications.
- **Owned Domains**:
  - *XDG Base Directory*: `$XDG_CONFIG_HOME`, `$XDG_DATA_HOME`, `$XDG_CACHE_HOME`, `$XDG_STATE_HOME`, `$XDG_RUNTIME_DIR`, and canonical fallback paths.
  - *XDG User Dirs*: Parsing `$XDG_CONFIG_HOME/user-dirs.dirs` for desktop, download, documents, music, pictures, videos folders.
  - *RFC 8089 File URIs*: Percent-encoding, URI authority verification, roundtrip conversion between canonical local paths and `file://` URIs.
  - *Shared MIME-info*: Magic byte sniffing, glob pattern matching (single-extension `*.rs`, multi-extension `*.tar.gz`), case-insensitive fallback, built-in common MIME mappings.
  - *Desktop Entries & mimeapps.list*: `.desktop` parser, Exec field code expansions (`%f`, `%F`, `%u`, `%U`), `mimeapps.list` default application associations with prioritized lookup.
  - *FreeDesktop Trash Specification v1.0*: Trash folder path resolution, `.trashinfo` metadata creation with RFC 3339 deletion timestamps and percent-encoded paths, collision handling (`item.1`), and restore routing.
  - *Clipboard Payloads*: RFC 2483 / standard `text/uri-list` and UTF-8 plain-text payload serialization and deserialization.
  - *RFC 1321 MD5*: Pure standard-library MD5 digest generator used to compute canonical Freedesktop thumbnail cache filenames.
- **Subsystem Boundary & Anti-Inversions**:
  - `arca-xdg` has strictly ZERO external dependencies (`std` only).
  - It must never depend on `arca-engine`, `arca-ui`, `arca`, or any crates.io crate.

### 2.2 `crates/arca-engine` (Application Domain Engine)
- **Role**: Headless domain engine and session state machine for file browsing, history, operations, and settings.
- **Owned Domains**:
  - *Directory Model*: `DirEntry` representations, filesystem metadata caching, file kind classification (file, directory, symlink, socket, fifo, device), hidden-file filtering.
  - *Navigation & Session State*: Multi-tab state management (`TabState`), per-tab current working directory, forward/backward history stacks with truncation on branch, bookmarks (GTK bookmarks, XDG user dirs, custom user pins), and an asynchronous read/poll seam (`begin_navigate` / `poll_navigation`) the interactive window uses so directory I/O never blocks a click frame.
  - *Sorting & Filtering*: Deterministic sorting (by Name, Size, Modified Time; ascending/descending) with directories pinned to top, case-insensitive substring search filter.
  - *File Operations (`ops`)*: Safe inline rename (refuses to clobber existing destinations), unique folder creation ("New Folder", "New Folder (1)"), batch trash movement, trash restoration, undo queue, clipboard copy/cut staging (`ClipboardAction`).
  - *Configuration (`config`)*: `~/.config/arca/arca.conf` parser and serializer (`key = value`) for view mode, sort order, hidden files flag, theme preference, thumbnails toggle, and bookmarks.
  - *Portal Chooser State Machine (`chooser`)*: Request/response serialization matching the Tessera prompter contract v6 and FileChooser v3 filters (globs, MIME types), choice controls (checkboxes, dropdown combos), overwrite protection guards.
  - *Metadata & Inspection (`preview`)*: Asynchronous, bounded text reading (head snippet inspection), directory size and entry count accumulation, image dimension parsing, audio metadata tags.
  - *Thumbnail Service*: Locates and hashes canonical file URIs according to Freedesktop thumbnail specifications (`~/.cache/thumbnails/normal/`, `~/.cache/thumbnails/large/`).
- **Subsystem Boundary & Anti-Inversions**:
  - `arca-engine` must NEVER depend on `arca-ui`, `optics-app`, `optics-ui`, `optics-window`, `optics-gfx`, or any GUI/windowing framework.
  - It depends only on `arca-xdg`, `serde`, `serde_json`, and `image` (minimal png/jpeg features for off-thread image inspection).
  - 100% of its functionality is testable in headless unit test runners without a display server.

### 2.3 `crates/arca-ui` (Presentation & Viewport Subsystem)
- **Role**: Presentation and interaction layer built on the Optics stack (`optics-app` application shell, `optics-ui` immediate-mode UI, `optics-window` platform runtime, `optics-gfx` Vulkan renderer, `optics-motion` motion dynamics).
- **Owned Domains**:
  - *Immediate-Mode Render Loop*: Builds the complete frame UI from `AppState` every frame without retaining parallel widget state trees. Arca is pure optics-ui chrome: it installs no host canvas recorder (`optics_app::RecordHost`) and would record an immutable DisplayList rather than touch a live canvas if a custom surface were ever added.
  - *Arena Memory Budget & Virtualization*: Enforces the 1 MiB per-frame widget arena budget of `optics-ui`. All listing views (**Grid**, **List**, **Miller columns**) slice the visible viewport window; only in-view items generate layout nodes and draw commands.
  - *Three View Modalities*:
    - **Grid View**: Responsive multi-column layout with icon/thumbnail tiles and centered text labels.
    - **List View**: Dense tabular rows with sortable column headers (Name, Size, Modified) and selection row highlights.
    - **Miller Columns View**: Parent · active · preview, with the active directory fixed in the centre column; the preview lists the directory under the cursor (read off-thread) or shows a file's details. Spec: [View Modes Reference](../reference/view-modes.md); rationale: ADR-0022.
  - *Tabs Management*: Multi-tab chrome, tab drag-reordering, active tab switching, tab close buttons, and keyboard shortcuts (`Ctrl+T`, `Ctrl+W`, `Ctrl+Tab`, `Ctrl+1..9`).
  - *Quick Look Preview*: Animated modal inspection overlay triggered by `Space`. Off-thread thumbnail/text rendering, smooth spring and cubic easing via Optics `optics-motion`, bounded memory usage.
  - *Floating Chrome & Modals*: Borderless surfaces adhering to the Tessera design language (`tessera-design`), subtle titlebar with window controls, appearance settings popup (System/Light/Dark), inline rename editing, batch trash confirmation dialogs.
  - *Icon Pipeline*: Bundled SVG icons (`assets/icons/*.svg`) decoded by Arca (`icons/vector.rs`) into optics-ui vector commands and registered per `optics_ui` context via `Ui::register_icon` (ADR-0025), GPU texture uploads via `optics_gfx_image_update_region` with `premultiply: true`.
  - *Headless Frame Verification*: Real frame generation tested via `optics_ui::Ui::headless()`, verifying layouts, keyboard actions, and arena budget without a Wayland compositor.
- **Subsystem Boundary & Anti-Inversions**:
  - `arca-ui` consumes domain state from `arca-engine`; it never executes disk mutations directly without delegating to `arca-engine::ops`.
  - Raw `optics_ui_sys` calls (widget focus, clipboard FFI, icon registration) are isolated inside centralized helper modules (`src/icons.rs`, `src/device.rs`, `src/thumbs.rs`).

### 2.4 `crates/arca` (Unified Executable Entry Point)
- **Role**: Unified binary entrypoint, CLI argument parser, and runtime posture selector.
- **Owned Domains**:
  - *Interactive Desktop Mode*: Initializes logging, loads config, instantiates `AppState`, registers icons, and launches the event loop via `optics_app::Application::run_with_lifecycle` with no host canvas recorder (pure optics-ui chrome).
  - *Portal Chooser Prompter Mode (`--chooser-prompt`)*: Standalone prompter process invoked by desktop portal daemons over stdio. Reads JSON request envelope from `stdin`, displays focused modal picker window, emits a single JSON response envelope to `stdout`, and terminates cleanly.
  - *Standalone CLI Picker Modes*: `--choose-file`, `--choose-files`, `--choose-dir`, `--save-file` returning selected path(s) to stdout for shell scripts.
- **Subsystem Boundary & Anti-Inversions**:
  - `arca` is a thin binary orchestrator; all reusable rendering logic resides in `arca-ui`, and all state logic resides in `arca-engine`.

---

## 3. Crate Dependency & Import Flow Matrix

The workspace dependency flow is strictly unidirectional from entrypoint to base specifications:

```text
arca (binary)
  ├── arca-ui
  │     ├── arca-engine
  │     │     ├── arca-xdg
  │     │     ├── image (png, jpeg)
  │     │     ├── serde, serde_json
  │     ├── arca-xdg
  │     ├── optics-app (application shell)
  │     ├── optics-window (window/platform)
  │     ├── optics-ui, optics-ui-sys
  │     ├── optics-gfx-sys
  │     └── optics-motion (pure Rust; no -sys crate)
  ├── arca-engine
  └── optics-app (lifecycle shell; relays the shared-branch rpaths)
```

| Source Crate | Permitted Dependencies | Forbidden Dependencies |
| :--- | :--- | :--- |
| **`arca-xdg`** | Rust `std` only | `arca-engine`, `arca-ui`, `arca`, external crates |
| **`arca-engine`** | `arca-xdg`, `image`, `serde`, `serde_json` | `arca-ui`, `arca`, any Optics crate (`optics-app`, `optics-gfx`, `optics-ui`, `optics-window`, `optics-text`, `optics-motion`, `optics`) |
| **`arca-ui`** | `arca-engine`, `arca-xdg`, Optics (`optics-app`, `optics-ui`, `optics-ui-sys`, `optics-window`, `optics-gfx-sys`, `optics-motion`) | `arca` (entrypoint) |
| **`arca`** | `arca-ui`, `arca-engine`, `optics-app` | Direct internal UI view modules |

---

## 4. Codified System Invariants

| Invariant ID | Name | Architectural Red-Line | Mechanical Enforcement |
| :--- | :--- | :--- | :--- |
| **`[INV-ARCH-01]`** | XDG Pure-Std Isolation | `arca-xdg` must have strictly zero external dependencies. | `architecture::xdg_layer_is_dependency_free` and CI job `architecture` |
| **`[INV-ARCH-02]`** | Engine Headless Purity | `arca-engine` must never link GUI or Wayland libraries. | `architecture::engine_layer_never_links_the_graphics_stack` and CI job `architecture` |
| **`[INV-ARCH-03]`** | Concentric Dependency Direction | Outer layers may depend inward only; no cycles, and production crates never depend on test infrastructure. | `architecture::dependency_direction_is_concentric`, `architecture::production_crates_do_not_depend_on_test_infrastructure` |
| **`[INV-UI-01]`** | 1 MiB Frame Arena Budget | Viewport rendering must never exceed the optics-ui 1 MiB arena memory limit. | `virtualization::large_directory_does_not_overflow_frame_arena_in_any_view`, `virtualization::scrolled_large_listing_builds_only_visible_rows` (CI job `test`) |
| **`[INV-UI-02]`** | Motion Math Centralization | Easing curves and spring dynamics must delegate to `optics-motion`. | Elimination of private approach/clamping math; ADR-0004 |
| **`[INV-UI-03]`** | Legible Text | Body text must meet WCAG AA (4.5:1) on its surface in both schemes. | `theme::contrast_tests::*` |
| **`[INV-UI-04]`** | Interaction Latency | A directory open must not request a synchronous XDG activation token or read the directory on the interactive frame thread; double-clicks are identified by entry absolute path; repetitive clicking must stay bounded and return to zero damage. | `app::tests::double_click_into_directory_does_not_panic_on_stale_rows`, `app::tests::double_click_does_not_cross_a_navigation_to_a_different_entry`, `state::tests::async_navigation*`, `ui_flows::asynchronous_navigation_*`, `observability_ui::interactive_navigation_keeps_blocking_reads_off_the_ui_thread`, `interaction_stress::*`, `damage_and_quiescence::*`; [Interaction Latency](interaction-latency.md), [ADR-0020](../adr/0020-asynchronous-directory-navigation.md) |
| **`[INV-OPS-01]`** | Non-Destructive Trash | Deletions must generate compliant FreeDesktop `.trashinfo` metadata, recording the deleted path itself (never a symlink's target). | `trash::tests::test_trash_roundtrip`, `trash::tests::trashing_a_symlink_records_the_link_path` |
| **`[INV-OPS-02]`** | Anti-Clobber File Creation | Renaming and folder creation must never silently overwrite destinations, including dangling symlinks. | `ops::tests::rename_refuses_to_clobber`, `ops::tests::rename_refuses_dangling_symlink_destination` |
| **`[INV-OPS-03]`** | No Self-Containment | A directory may never be copied or moved into itself or a descendant. | `ops::tests::copy_and_move_reject_self_descendant_targets`, `state::tests::drop_refuses_a_directory_into_its_own_descendant` |
| **`[INV-OPS-04]`** | Non-Blocking Special Files | FIFOs, devices and sockets are never opened for content. | `preview::tests::preview_of_a_fifo_returns_immediately` |
| **`[INV-OPS-05]`** | Reaped Launches | Every process Arca launches is waited on; opening files never accumulates zombies. | `desktop::tests::detached_children_are_reaped` |
| **`[INV-OBS-01]`** | Protocol-Clean Logging | The logger never writes stdout; the prompter's stdout carries exactly one protocol line at any `ARCA_LOG` level. | `diagnostics::prompter_logs_to_the_state_dir_and_keeps_stdout_protocol_clean` (e2e) |
| **`[INV-OBS-02]`** | Freeze Evidence | A UI frame running past `ARCA_STALL_MS` raises `ui_stall` naming the innermost UI span, and its completion raises `ui_stall_recovered`; an idle window never stalls. | `watchdog::tests::*`, `observability::a_stalled_ui_thread_is_alerted_with_the_blocking_operation`; [ADR-0021](../adr/0021-local-first-observability.md) |
| **`[INV-NAV-01]`** | Deferred History | Back/Forward move the history cursor only when the target listing publishes; rapid clicks stack. | `state::tests::a_failed_async_back_leaves_the_history_cursor_in_place`, `state::tests::rapid_async_back_clicks_walk_history_and_publish_once` |
| **`[INV-DATA-01]`** | Selection Identity | Selection is name-based and always a subset of the visible set; the cursor is always selected. | `regression_tests::refresh_keeps_the_selection_highlight`, `regression_tests::sorting_keeps_the_selection_on_the_same_file` |
| **`[INV-DATA-02]`** | Transfer Payload Parity | Drops accept the same interchange formats the clipboard emits (`text/uri-list`, `x-special/gnome-copied-files`, bare paths). | `state::tests::drop_transfers_every_item_in_a_uri_list_payload`, `clipboard::tests::*` |
| **`[INV-DOC-01]`** | Code-to-Doc Parity | Every CLI flag, config key, sort value and key binding appears in its reference. | `scripts/check-docs.py` (CI job `docs`) |
| **`[INV-CLI-01]`** | Unified Binary Singleton | `arca` is the sole public executable binary. | Cargo workspace root manifest binary configuration |
| **`[INV-BUILD-01]`** | Release Link Mode | The *release* `arca` binary must embed the Optics native stack: no `liboptics_*.so` `DT_NEEDED` entry, no Optics runpath, and `optics_app_run` in the symbol table (or, for a stripped artifact, surviving `optics_*` references). Linked `dev` worktrees override to dynamic via `[env] OPTICS_DYNAMIC = "1"`. | Workspace enables `features = ["static"]` by default; CI job `test` gates `cargo build --release -p arca` with `scripts/check-optics-link.sh --expect static` and `OPTICS_DYNAMIC=1` with `--expect dynamic`; ADR-0014, amended by ADR-0015 and ADR-0016 |
