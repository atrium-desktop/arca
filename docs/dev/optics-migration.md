# Optics migration

Record of Arca's port from the legacy hand-written stack to the
[optics](../../../optics) monorepo (`optics-app` / `optics-gfx` / `optics-ui` /
`optics-window` / `optics-motion`), and of the resulting workspace
architecture.

Optics `v0.0.82` renamed every module to its semantic name (Optics ADR-0169,
ADR-0170, ADR-0172): the codenames below (`aperture`, `flux`, `lens`, `iris`,
`reticle`, `transit`) survive only in historical documents and in the
per-release evolution log at the end of this page. Optics `v0.0.83` then made
the Rust link mode shared-preferred by default and added an additive `static`
feature; Arca repinned and kept its release pin explicit (ADR-0015).

## Before → after

| Concern | Legacy (pre-0.2) | Now |
|---------|------------------|-----|
| Window, Wayland, event loop | Hand-written C shim (`c_src/platform.c`) generating xdg-shell / xdg-decoration / text-input protocol code in `build.rs` | `optics-window` owns the window, GPU device and event loop; the `optics-app` shell runs the `optics-ui` interface inside it |
| Application shell | Hand-wired `Ui::run` loop | `optics_app::Application` with start/stop lifecycle and an optional host recorder |
| UI toolkit | `flux-ui` via hand-written FFI (`src/ffi.rs`, icon ids as raw integers) | `optics-ui` safe bindings, plus narrow `optics_ui_sys` helpers for the full icon set / focus / clipboard |
| Layout | single crate, `app.rs` + `fs.rs` | workspace: `arca-engine` (logic) · `arca-ui` (view) · `arca` (bin) |
| Theme | manual dark toggle only | follows the system colour scheme live; System → Light → Dark override, persisted |
| Settings | none | `~/.config/arca/arca.conf` (hidden files, sort, theme, bookmarks) |

The legacy C shim and FFI were deleted wholesale; no code was carried over
unchanged, but the interaction model (toolbar / sidebar / list / status
bar) and icon mapping were ported deliberately.

## How the binding works

- The workspace declares tagged Git dependencies on `optics-app`,
  `optics-window`, `optics-ui`, `optics-ui-sys`, `optics-gfx-sys`, and
  `optics-motion`; the local patch template redirects them to
  `../optics/crates/optics-{app,window,ui,ui-sys,gfx-sys,motion}`.
  The safe `optics-gfx` crate is not declared directly: it arrives
  transitively through `optics-app` and `optics-ui`, while Arca calls
  `optics-gfx-sys` only for thumbnail image uploads.
  Each `*-sys` `build.rs` walks up from its manifest, finds the optics
  checkout (presence of `libs/<component>/include/optics` +
  `build/meson-uninstalled`), and links the meson build tree through the
  `optics-*-uninstalled.pc` pkg-config files — no `meson install` required.
- `optics-app` re-publishes its library directories as
  `links = "optics_app_rs"` rpath metadata, `optics-window` as
  `links = "optics_window_rs"`, and `optics-ui` as `links = "optics_ui_rs"`.
  Arca follows the shared-preferred default (Optics v0.0.83), so `arca-ui`
  relays all three lists and `arca` relays the `optics-app` list into
  `-Wl,-rpath` arguments (with `--disable-new-dtags` so transitive deps
  resolve), which lets binaries and test executables run without
  `LD_LIBRARY_PATH`. `OPTICS_STATIC=1` (or the `static` feature) embeds the
  `liboptics_*.a` archives instead and emits no rpath. `optics-motion` is pure
  Safe Rust since Optics `v0.0.79` (Optics ADR-0158), so it has no native
  library and no rpath relay.
- Where the safe `optics-ui` surface is intentionally narrow, `arca-ui`
  drops to `optics_ui_sys` via `frame.as_raw()` inside dedicated helpers:
  the full feather icon set (`icons.rs`), label-scoped focus
  (Ctrl+L / F3 / inline rename), and clipboard copy (`menus.rs`).
  Focus labels are resolved by `optics_ui_current_id` immediately after the
  target widget is built, while that widget's id scope is current; no
  numeric widget id is cached across frames.

## State and input flow

`arca_engine::AppState` is the single source of truth (cwd, entries,
history, selection, filter, clipboard, bookmarks, config). `arca-ui`
renders it every frame and calls its methods on input. Key routing rule:
while any text field owns focus, all keys except `Return` pass through to
the field — app shortcuts only fire when no field is focused.

Rename is inline (the row's name swaps to a text field): `Return` commits,
focus loss cancels. `Esc` is reserved by `optics-window` for quitting.

## Testing

- `arca-engine`: unit tests for sorting, filtering, formatting, history,
  path handling, config round-trip, trash naming/encoding, copy/move —
  all headless with temp-dir fixtures.
- `arca-ui`: smoke tests drive real frames through `optics_ui::Ui::headless()`
  (frame builds, `Ctrl+H` toggles hidden files, arrow+return navigation,
  filter application).
- Manual: `cargo run` inside a Wayland session.

## Evolution After v0.2.0

Following the initial 0.2.0 release:
1. **Workspace Modularization (v0.2.2)**: `arca-core` was formally split into `crates/arca-xdg` (pure-std Freedesktop specifications: Base Dirs, User Dirs, RFC 8089 URIs, MIME sniffing, `.desktop`, and FreeDesktop Trash v1.0) and `crates/arca-engine` (application state machine, navigation, file operations, configuration, thumbnail caching).
2. **Viewport Virtualization**: All directory listing views (**Grid**, **List**, and **Miller columns**) were virtualized to guarantee execution within the `optics-ui` 1 MiB immediate-mode frame arena budget (`tests::large_directory_does_not_overflow_frame_arena`).
3. **Motion Dynamics & Optics v0.0.60 Alignment (ADR-0004)**: Adopted Optics motion (now `optics-motion`) for centralized cubic easing and spring dynamics in Quick Look modal transitions, and hardware-accelerated premultiplied thumbnail uploads via the `optics_gfx_image_update_region` descriptor.
4. **Regional Damage & Quiescence (Optics v0.0.74, ADR-0008)**: Adopted the bounded regional damage rect list, departure damage for removed rows, host-controlled caret/selection placement, and zero-damage idle presentation.
5. **Native Frame Damage & Caret Phases (Optics v0.0.76, ADR-0009)**: Delegated the per-image damage epoch ledger to the window/gfx layers, moved focus to build-site label resolution, disabled the caret blink under reduced motion, and verified scrolling viewport damage and caret-flip-only repaints.
6. **Record-Only Host Canvas (Optics v0.0.78, ADR-0010)**: Retired the deleted `PaintHost`/live-paint contract, kept both entry points on the pure-chrome record seam with an empty `RecordHost`, and audited the gfx snapshot, dynamic-slot, and DisplayList wire contracts against Arca's write-once thumbnail ownership.
7. **Unified Bindings & Pure-Rust Motion (Optics v0.0.79, ADR-0011)**: Moved the local patch to one bindings workspace, retired the native motion FFI crate and its rpath relay, and kept icon registration on `optics-ui` so every shipped SVG compiles through the shared vector compiler without an application-side duplicate.
8. **Application Shell & Descriptor-Only Operations (Optics v0.0.80, ADR-0012)**: Moved the application lifecycle into the separate application-shell library, split UI imports onto the widget crate and platform control onto the window crate, and adopted the final scroll and image-update descriptors. The removed graphics CPU/MSAA/supersampling paths had no Arca call sites.
9. **Semantic Module Names (Optics v0.0.82, ADR-0013)**: Adopted the `optics-<module>` crate names and unified `optics_<module>_` symbols (Optics ADR-0169, ADR-0170, ADR-0172), moved the local patch to the repository-root `../optics/crates/` tree, and re-derived the rpath relay from the `optics_app_rs`/`optics_window_rs`/`optics_ui_rs` `links` keys. Public C entry points updated: `optics_ui_*` for widgets and icons, `optics_window_*` for platform control, `optics_gfx_*` for thumbnail images, and `optics_app_*` for the lifecycle.
10. **Optics Link Mode (ADR-0014)**: Kept link selection as a build input — `OPTICS_STATIC=1`, `OPTICS_DYNAMIC=1`, or the upstream default of the pinned release — and stopped treating the *outcome* as an invariant of every build. The `arca` and `arca-ui` build scripts only relay the rpaths that the shared branch publishes; the release artifact is pinned to static by the release invocation and verified with `scripts/check-optics-link.sh --expect static` in the CI job `test`.
11. **Shared-Preferred Linking (Optics v0.0.83, ADR-0015)**: Repinned the workspace to `v0.0.83`, whose `optics-*-sys` crates link `liboptics_*.so` by default and expose an additive `static` Cargo feature; `OPTICS_STATIC` / `OPTICS_DYNAMIC` remain the top-priority overrides. Arca's development builds now follow the shared default through the rpath relay while the release artifact still pins `OPTICS_STATIC=1`. Adopted the `optics_app::window` and `optics_ui::GfxDiagnostic` renames (no Arca call site used the retired `iris` / `FluxDiagnostic` names) and noted the Optics ADR-0173 retirement of the shader fallback, earcut vault, and upper-case `OPTICS_GFX_INIT` aliases, none of which Arca called.
