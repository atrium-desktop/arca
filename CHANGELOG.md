# Changelog

All notable changes to Arca will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

---

## [0.2.16] - 2026-11-23

### Added
- Synchronized workspace dependencies with Optics `v0.0.83` and landed ADR-0015, which amends ADR-0014 for the shared-preferred link default.
- Adopted the semantic module names and unified `optics_` namespace (Optics ADR-0169, ADR-0170, ADR-0172): the workspace now depends on `optics-app`, `optics-window`, `optics-ui`, `optics-ui-sys`, `optics-gfx-sys`, and `optics-motion`.
- Local development patches now resolve the repository-root `../optics/crates/` workspace, including the shared `optics-build` helper.
- `scripts/check-optics-link.sh` reports which Optics link mode a binary actually used and asserts it with `--expect static|dynamic`; the CI job `test` gates the release artifact on `--expect static` and the shared build on `--expect dynamic` (`[INV-BUILD-01]`).
- `scripts/test-check-optics-link.sh` covers the gate's own failure modes: a non-Optics binary is rejected, a fully stripped artifact needs `--allow-unverified-stripped`, bad arguments exit 2, and the announced mode is accepted while the opposite one is not. The runpath check now flags any directory that holds `liboptics_*.so`, not only paths containing `optics`.

### Changed
- Optics link selection stays a **build input** (ADR-0014): `OPTICS_STATIC=1` requires the `liboptics_*.a` archives, `OPTICS_DYNAMIC=1` forces the `liboptics_*.so` objects, the additive `static` Cargo feature requests the archives, and leaving all unset uses the upstream **shared-preferred** probe introduced in Optics `v0.0.83`. `crates/arca` and `crates/arca-ui` do not enable the feature and reject neither mode; they only relay the rpaths that the shared branch publishes. The release artifact is pinned to static by the release invocation, so `target/release/arca` carries no `liboptics_*.so` dependency and no build-tree runpath while development builds default to the shared objects. The process image otherwise stays dynamic (`libc`, Vulkan loader, Wayland, font stack).
- Adopted the Optics `v0.0.83` Rust renames: `optics_app::iris` is now `optics_app::window` and `optics_ui::FluxDiagnostic` is now `GfxDiagnostic`. Arca imports the module paths and pattern types directly, so no call site changed.
- Building the pinned Optics C stack now requires a C23 compiler with `#embed` support (GCC >= 15, Clang >= 19); Arca's Rust toolchain floor stays at 1.85.
- Packaging loses its Optics runtime dependency: the archives are a build-time requirement (`-Ddefault_library=both`), the recipes build with `OPTICS_STATIC=1`, add `binutils` to the build dependencies, and assert `check-optics-link.sh --expect static` before stripping.
- Renamed every Optics import to the owning module: `optics_ui::*` widgets and patterns, `optics_window::*` platform control, `optics_app::*` lifecycle (`Application`, `Config`, `RunError`, `StartHost`, `StopHost`, `RecordHost`), `optics_motion::*` springs, and `optics_gfx_sys::optics_gfx_*` thumbnail images.
- Migrated the narrow FFI helpers to the stutter-free C surface: `optics_ui_sys::{optics_ui_scroll_offset, optics_ui_scroll_to, optics_ui_set_focus, optics_ui_current_id, optics_ui_set_caret_blink, optics_ui_reduced_motion, optics_ui_button}`, the `optics_ui_icon_id`/`optics_ui_id` newtypes, and `OPTICS_UI_BUTTON_SUBTLE` / `OPTICS_UI_ICON_COUNT`.
- `StartHost::flux_device()` became `StartHost::optics_gfx_device()`; the icon adapter's `lens_id` became `icon_id` to match the module vocabulary.
- Rpath relay now reads the `optics_app_rs` / `optics_window_rs` / `optics_ui_rs` `links` metadata, restoring local test binaries without `LD_LIBRARY_PATH`.
- Updated the headless-engine firewall, packaging `pkg-config`/`BuildRequires`/`ldd` lists, setup, testing, acceptance, and glossary references to the semantic module names.

---

## [0.2.15] - 2026-11-16

### Added
- Synchronized workspace dependencies with Optics `v0.0.80` and landed ADR-0012.
- Adopted the `aperture` application shell (Optics ADR-0166): `Application`, `Config`, `RunError`, and the start/stop/record hosts now come from aperture, while Lens owns UI/input types and Iris keeps window, cursor, dialog, and theme control.

### Changed
- Moved every `iris::{Frame, Input, Color, Pattern}` import to its owning Lens crate; only genuine platform calls (`window_close`, `request_animation_frame`, `post_to_main_thread`, `system_prefers_dark`) remain on Iris.
- Adopted the single-option descriptors of Optics ADR-0165 in the remaining direct FFI calls: `lens_scroll_offset` / `lens_scroll_to` and `flux_image_update_region`.
- Thumbnail uploads now initialize `flux_image_desc.usage` explicitly and premultiply through `flux_image_update_region_opts`, replacing the retired `flux_image_update_region_premultiply` entry point.
- Native rpath relay now merges aperture and iris metadata so both lifecycle hosts find the uninstalled Optics libraries without `LD_LIBRARY_PATH`.
- Audited the removed Flux canvas creation paths (CPU anti-aliasing, supersampling, MSAA, no-stencil passes, tessellator); Arca never called them and remains on sampled thumbnail images only.
- Dropped the unused direct `flux` workspace entry; `aperture`/`lens` still pull it transitively, while Arca continues to call `flux-sys` for image uploads.

---

## [0.2.14] - 2026-11-12

### Added
- Synchronized workspace dependencies with Optics `v0.0.79` and landed ADR-0011.
- Adopted the unified `bindings/crates/` workspace (Optics ADR-0159) in the canonical manifests and the local `[patch]` template.
- Adopted the header-only, pure-Rust `transit` motion vocabulary (Optics ADR-0158); springs and easing now compile directly into Arca.

### Changed
- Removed the retired `transit-sys` crate and the `DEP_TRANSIT_*_RPATHS` handling from `arca-ui` and `arca`; only the Iris native rpath relay remains.
- Icon registration stays on `lens::register_svg_icon`, which now parses every shipped glyph through the shared `reticle` compiler (Optics ADR-0161); no application-side SVG parser or direct `reticle` dependency was added.

---

## [0.2.13] - 2026-11-12

### Added
- Synchronized workspace dependencies with Optics `v0.0.78` and landed ADR-0010.
- Adopted the record-only host canvas seam (Optics ADR-0127): the live-canvas `paint` callback, `PaintHost`, and `PaintCanvas` are gone, so both Arca entry points pass an empty `RecordHost` and remain pure Lens chrome.
- Audited the v0.0.78 Flux ownership contracts (RenderPlan, DisplayList wire v1, dynamic image slots, immutable snapshots, dma-buf leases) and kept Arca on owned write-once thumbnail images; no new per-frame ownership or serialization layer was introduced.
- Synchronized workspace dependencies with Optics `v0.0.76` and landed ADR-0009.
- Adopted the native frame damage epoch ledger (Optics ADR-0149 / ADR-0155) through Iris: Arca keeps no per-image damage ledger; regional damage and quiescence are verified for virtualized scrolling, row removal, modal preview transitions, and segmented view switches.
- Adopted label-scoped host focus (Optics v0.0.76): location, filter, and inline rename focus targets resolve at the widget's build site instead of caching numeric ids across frames.
- Adopted caret phase semantics and reduced-motion policy (Optics ADR-0151): a focused field reports damage only on blink flips, and reduced motion disables the blink clock with a solid caret.
- Re-validated every shipped SVG icon against the new `libsvg` compiler path; the icon registration suite fails if any glyph stops parsing.
- Synchronized workspace dependencies with Optics `v0.0.74` and landed ADR-0008.
- Adopted Optics regional damage rect list contract (`Ui::damage_rects`, Optics ADR-0143 / ADR-0149) with integration verification in `crates/arca-ui/tests/damage_and_quiescence.rs`.
- Departure damage tracking for departed nodes (`LENS_NODE_LEAVING`): row deletion in directory listings reliably invalidates and clears the vacated area without stale pixels or full-viewport redraws.
- Demand-driven presentation quiescence: verified zero-damage sleeping and animation termination for idle frames and segmented view switcher transitions (Optics ADR-0151, ADR-0152).
- Host-controlled selection and caret positioning (`Frame::textedit_set_selection` and `Frame::textfield_set_caret`): location bar focus (`Ctrl+L`) auto-selects all text, and inline file rename (`F2`) automatically pre-selects the filename stem, preserving extensions.
- Consolidated developer runbook into `docs/dev/build-and-run.md` conforming to Optics ADR-0125 documentation standards.

### Fixed
- Local-build runpaths now find the full-version `libtransit.so.0.0.78` SONAME by exposing `transit-sys` links metadata to the `arca-ui` and `arca` build scripts.
- Headless UI tests no longer create their isolated config inside the directory under test, and concurrent fixtures get a unique serial suffix; persisted settings can no longer shift visible entry indices mid-suite.

## [0.2.12] - 2026-10-31

### Added
- Synchronized workspace dependencies with Optics `v0.0.66` and landed ADR-0007.
- Adopted `iris::patterns::popup_menu` and `MenuItem` for context menus (`build_ctx_menu`, `build_sidebar_menu`) featuring full vector iconography, right-aligned shortcuts, and keyboard navigation (Up/Down arrow, Return, Escape).
- Adopted `Response::submitted` from `Frame::textfield_ex` for declarative Return key submission in inline file renaming and file chooser save-name fields.
- Native multi-level undo/redo history (`Ctrl+Z`, `Ctrl+Shift+Z`, `Ctrl+Y`) and action coalescing across all text inputs from Optics textedit enhancements.
- Adopted Optics compound patterns (`DataGrid`, `TreeView`, `menubar`, `popup_menu`) via direct root exports in `iris::patterns`.
- Synchronized workspace dependencies with Optics `v0.0.64` and landed ADR-0006.
- Adopted orthogonal icon sizing (`Theme::with_icon_size(18.0)`) and `Frame::selectable_icon_sized` from Optics ADR-0111, eliminating subpixel icon smearing and preserving distinct visual weights between typography and iconography.
- Direct root re-export of `SegmentedControl` and `SegmentedItem` via `iris`.
- Automated subtle button corner radius refinement conforming to Tessera standards.
- Hardware display-paced presentation loop and subsurface compositing alignment via Optics ADR-0122 – ADR-0124.

### Changed
- Standard desktop arrow cursors (`LENS_CURSOR_DEFAULT`) natively enforced on selectable items without manual post-widget cursor resets.
- Removed deprecated `icon_button_active_rounded` helper in `arca-ui::icons`.
- Hardened kernel inotify instance budget exhaustion handling in filesystem watcher test suites.
- **File transfer contract unified.** Clipboard export/import and drag-and-drop now share
  `arca_xdg::FileClipboard`, so the two paths cannot drift. The engine's duplicate
  `Clipboard` type was deleted; `TransferAction` replaces the `cut: bool` flag that made
  a destructive inversion a one-character mistake.
- **Selection is modelled by entry name, not visible index.** `TabState` keeps
  authoritative `selected_names` / `cursor_name` / `anchor_name` and derives the index
  caches, so re-reads, re-sorts, hidden-file toggles and filter edits can no longer move
  the highlight onto a different file.
- The listing filter uses each entry's already-resolved MIME type instead of
  re-sniffing from disk: `visible()` under a MIME filter drops from ~12.9 ms to ~0.5 µs
  for 2400 entries.
- Miller column models are built only in Miller view; navigating in List/Grid no longer
  re-reads four ancestor directories (~10 ms) for a model nothing renders.
- Transfers ≥ 8 MiB run on the existing background IO worker, which is now actually
  driven: progress appears in the status bar and completion commits one undo
  transaction. Small transfers keep the synchronous path.
- `AppState::toggle_hidden` refreshes each tab in place instead of temporarily
  reassigning `active_tab`.

### Fixed
- **Copying or moving a directory into itself recursed until `ENAMETOOLONG`,** leaving
  hundreds of nested copies on disk. Both endpoints are now canonicalized and the
  transfer is refused before any I/O.
- **A multi-item drag or an external drag transferred nothing**: the raw `text/uri-list`
  payload was handed to `PathBuf::from`. Drops now parse the interchange format and
  transfer every item.
- **Quick Look could not be dismissed with `Esc` or a click outside**, both of which are
  documented. The overlay is registered in lens's transient open-set and its dismissal is
  mirrored back into app state.
- **Selection highlight vanished after any refresh** (`Ctrl+R`, `F5`, or any inotify
  event) because only the cursor index was restored.
- **`multiple: true` file-chooser requests returned a single path** (or none): the staged
  selection set was never written. Multi-select now stages on click and `Ctrl`-click and
  returns the whole set; single mode still refuses to return more than one path.
- **Grid file names were unreadable in the light theme** (1.30:1 at rest, 1.04:1 on
  hover). Label colours are now theme tokens, and WCAG AA contrast is enforced by tests.
- **Previewing a FIFO, device node or socket hung the UI thread.** Non-regular files are
  detected and described instead of opened; the thumbnail decoders gained the same guard.
- **Deleting a symlink to a directory recursively deleted the target's contents.**
  `delete_path` now unlinks the link itself.
- **Renaming onto a dangling symlink silently replaced it.** Destination existence is
  probed with `symlink_metadata`.
- **Trashing a symlink recorded its target's path,** so restoring put it in the wrong
  directory.
- **The sidebar's Trash bookmark opened `…/Trash`** (two bookkeeping directories) instead
  of `…/Trash/files`, and the restore branch was therefore unreachable. The listing now
  shows each item's original directory and deletion date, and offers Restore / Delete
  permanently / Empty Trash.
- `Ctrl+W` on the last tab did nothing despite being documented as closing the window;
  it now requests a window close. `Ctrl+Q` is bound.
- `sort = modified` (documented) parsed to `None` and silently fell back to name order.
- The prompter accepted any contract version; it now rejects mismatches, and validates
  its own response against the request before emitting it.
- Dropping a non-file payload now reports "no usable file paths" instead of a bogus
  `ENOENT` for a truncated path.
- `count_path` in the IO worker no longer follows symlinked directories.

### Engineering structure
- **`arca-ui` test suites migrated out of `lib.rs`.** The single 1400-line test
  module became per-module suites (`app`, `content`, `tabs`, `toolbar`, `theme`,
  `icons`, `menus`, `sidebar`, `preview`, `chooser`, `statusbar`, `thumbs`, `ipc`),
  sharing one `test_support` harness with an RAII `ScratchDir`. `lib.rs` now holds
  zero tests, and `scripts/check-docs.py tests` fails if a substantial module lacks
  a suite or the root block grows past 12 tests.
- **`arca` argument parsing extracted to `src/cli.rs`** as a pure
  `Command::parse(args, forward_probe)`, with 11 tests covering every flag
  combination. `main` now only performs the described side effects.
- **`UndoStack` gained its first tests** (6) — the module had none despite owning
  undo/redo semantics for every destructive operation.
- `statusbar`'s summary is a pure function, tested directly rather than through the
  widget tree.
- `ipc` gained 5 tests; the pending-request queue is now an `IpcServer` field
  instead of a process-global static (the global made two windows in one process
  share a mailbox, and made the tests race).

### Fixed (found while auditing the refactor itself)
- **`arca --grid` / `--list` / `--miller` silently rewrote the user's saved
  default.** The CLI path had been changed to call `set_view_mode`, which persists;
  a launch-time flag must not become a preference. Split into
  `set_view_mode` (persists, used by the toolbar) and `override_view_mode`
  (session only, used by the CLI), with a test asserting the config file is
  byte-identical after an override.
- `--help` omitted `--prompter`, `-h` and `-V`; `scripts/check-docs.py cli` now
  cross-checks the parser, the usage text and `docs/reference/cli.md` as three
  copies of the same list, and found the gap immediately.
- Removed two `#[allow(dead_code)]` items that existed only to keep unused API
  alive (`theme::label_centered`, `content::GRID_DEFAULT_ROW_PITCH` — the latter
  was only ever asserted equal to its own definition).

### Fixed (found while writing the missing tests)
- **The status bar summed directory inode sizes into the selection total**: a
  folder rendered as `—` in the Size column but reported its 41-byte inode length
  in the aggregate. Only regular files contribute now.
- `IpcServer::drop` is the sole socket owner and unlinks on every exit path
  (window close, `Ctrl+W` on the last tab, `Ctrl+Q`); `UiApp` owns the endpoint.
- Redo of a copy reported success while discarding the action.

### Documentation
- Corrected drift across the CLI, configuration, shortcut, Quick Look, trash, thumbnail
  and architecture references (cache paths, feature claims, key bindings, dedup suffixes,
  filename pre-selection, `trash:///` navigation, stale `arca-core` / `views/mod.rs`
  references).
- Added `.github/workflows/ci.yml` with architecture-firewall, doc-sync, clippy
  (`-D warnings`), test and named-invariant gates, plus `scripts/check-docs.py` which
  derives CLI flags, config keys and key bindings from source and asserts the reference
  documents them.

---

## [0.2.11] - 2026-10-18

### Added
- Fluid adaptive grid architecture and dynamic typographic cell metrics (ADR-0005).
- Multi-script line height measurement (Latin + CJK) ensuring mathematical clearance across all languages.
- Fluid column width allocation via `virtual_grid_calc` (104px–144px range) to eliminate dead margins and increase readable filename space.
- Fixed 50px art container ensuring stable horizontal baseline alignment across varying aspect ratios.
- Comprehensive automated regression and edge case test suites for grid virtualization.

### Fixed
- Fixed critical file name and picture preview overlay defect in grid view where 2-line labels collided with subsequent row thumbnails.
- Fixed hit-box desynchronization where clicking the second line of text dispatched events to the subsequent row.

---

## [0.2.10] - 2026-10-17

### Added
- Comprehensive engineering standards suite in `docs/dev/standards/` (`architecture.md`, `invariants.md`, `adr.md`, `rfc.md`, `taxonomy.md`, `style.md`, `workflow.md`, `postmortems.md`).
- Living subsystem architecture blueprint in `docs/dev/subsystems.md`.
- Manual testing runbook with cold-start sandboxing in `docs/dev/manual-testing.md`.
- Lockstep crate versioning guide in `docs/dev/release-and-versioning.md`.
- Chinese documentation (`README-zh.md`).
- ADR-0004: Alignment with Optics v0.0.60 motion dynamics (`transit`) and hardware premultiplied image upload pipeline.

### Changed
- Replaced nested governance directories with a flat, cohesive standards suite under `docs/dev/standards/`.
- Modernized `AGENTS.md` with explicit operational guardrails, mental model & boundaries, engineering discipline, and on-demand standards routing.
- Adopted Optics `transit` for spring dynamics and cubic easing in Quick Look animations.
- Refactored thumbnail GPU texture uploads to delegate premultiplication to `flux_image_update_region_premultiply`.

---

## [0.2.9] - 2026-09-28

### Added
- Tab drag-and-drop reordering with smooth visual positioning feedback.
- Clean application lifecycle shutdown handling integrated with `iris`.
- Synchronized workspace dependencies with Optics `v0.0.46`.

---

## [0.2.8] - 2026-09-18

### Added
- Optics extended function key navigation bindings.
- Wayland window activation token support (`iris::window_create_activation_token`) for focus stealing prevention.
- Refined focus management across location bar and inline rename editors.

---

## [0.2.7] - 2026-09-10

### Added
- Borderless floating chrome and subtle titlebars adhering to the Tessera design language (`ADR-0003`).
- Recessed input surfaces for location bar and search filter.
- Vibrant, crisp SVG icon set registered with Lens at runtime.

---

## [0.2.6] - 2026-09-05

### Added
- Live filesystem watching and automatic directory listing refresh on external changes.
- Default MIME application associations via `mimeapps.list` and `.desktop` parsing.
- Context menu stability improvements and hover interaction polish.

---

## [0.2.5] - 2026-08-30

### Added
- Location bar tab-completion for filesystem paths.
- Drag-and-drop file operations support.
- Improved contrast and typography for floating popovers and dialogs.

---

## [0.2.4] - 2026-08-25

### Added
- XDG Desktop Portal FileChooser label mnemonics parsing (`ADR-0002`).
- Fixed virtualized scroll boundary calculation in large directories.

---

## [0.2.3] - 2026-08-22

### Added
- Portal prompter JSON envelope updates matching Tessera Prompter Contract v6.
- Overwrite protection modal in file save dialogs.

---

## [0.2.2] - 2026-08-21

### Changed
- Major workspace restructuring: split legacy `arca-core` into pure-std `arca-xdg` and headless `arca-engine`.
- Implemented pure-std Freedesktop.org specifications: Base Directories, User Directories, RFC 8089 File URIs, Shared MIME-info, Desktop Entries, and FreeDesktop Trash v1.0.

---

## [0.2.1] - 2026-08-20

### Changed
- Bumped Optics dependencies to `v0.0.33`.
- Improved build script rpath relaying for native C library discovery without `LD_LIBRARY_PATH`.

---

## [0.2.0] - 2026-08-20

### Added
- Initial port from legacy C platform shim and `flux-ui` to the Optics monorepo (`flux`, `lens`, `iris`) (`ADR-0001`).
- Concentric Cargo workspace layout.
- Immediate-mode UI layout with 1 MiB frame arena memory budgeting and viewport row virtualization.
