# Optics migration

Record of Arca's port from the legacy flux/flux-ui stack to the
[optics](../../../optics) monorepo (flux / lens / iris), and of the
resulting workspace architecture.

## Before → after

| Concern | Legacy (pre-0.2) | Now |
|---------|------------------|-----|
| Window, Wayland, event loop | Hand-written C shim (`c_src/platform.c`) generating xdg-shell / xdg-decoration / text-input protocol code in `build.rs` | `iris` (official Rust binding) owns window, GPU device and event loop |
| UI toolkit | `flux-ui` via hand-written FFI (`src/ffi.rs`, icon ids as raw integers) | `lens` safe bindings, plus narrow `lens_sys` helpers for the full icon set / focus / clipboard |
| Layout | single crate, `app.rs` + `fs.rs` | workspace: `arca-core` (logic) · `arca-ui` (view) · `arca` (bin) |
| Theme | manual dark toggle only | follows the system colour scheme live; System → Light → Dark override, persisted |
| Settings | none | `~/.config/arca/arca.conf` (hidden files, sort, theme, bookmarks) |

The legacy C shim and FFI were deleted wholesale; no code was carried over
unchanged, but the interaction model (toolbar / sidebar / list / status
bar) and icon mapping were ported deliberately.

## How the binding works

- The workspace declares path dependencies on
  `../optics/bindings/crates/{iris,lens,lens-sys}`. `iris-sys`'s `build.rs` walks
  up from its manifest, finds the optics checkout (presence of
  `libs/iris/include` + `build/meson-uninstalled`), and links the meson
  build tree through the `*-uninstalled.pc` pkg-config files — no
  `meson install` required.
- The `iris` crate re-publishes the resulting library directories as
  `links = "iris_rs"` rpath metadata. Both `arca-ui` and `arca` carry
  a small `build.rs` relaying `DEP_IRIS_RS_RPATHS` into
  `-Wl,-rpath` arguments (with `--disable-new-dtags` so transitive deps
  resolve), which lets binaries and test executables run without
  `LD_LIBRARY_PATH`. `transit` is pure Safe Rust since Optics `v0.0.79`
  (Optics ADR-0158), so it has no native library and no rpath relay.
- Where the safe `lens` surface is intentionally narrow, `arca-ui`
  drops to `lens_sys` via `frame.as_raw()` inside dedicated helpers:
  the full feather icon set (`icons.rs`), label-scoped focus
  (Ctrl+L / F3 / inline rename), and clipboard copy (`menus.rs`).
  Focus labels are resolved by `lens_current_id` immediately after the
  target widget is built, while that widget's id scope is current; no
  numeric widget id is cached across frames.

## State and input flow

`arca_core::AppState` is the single source of truth (cwd, entries,
history, selection, filter, clipboard, bookmarks, config). `arca-ui`
renders it every frame and calls its methods on input. Key routing rule:
while any text field owns focus, all keys except `Return` pass through to
the field — app shortcuts only fire when no field is focused.

Rename is inline (the row's name swaps to a text field): `Return` commits,
focus loss cancels. `Esc` is reserved by iris for quitting.

## Testing

- `arca-core`: unit tests for sorting, filtering, formatting, history,
  path handling, config round-trip, trash naming/encoding, copy/move —
  all headless with temp-dir fixtures.
- `arca-ui`: smoke tests drive real frames through `lens::Ui::headless()`
  (frame builds, `Ctrl+H` toggles hidden files, arrow+return navigation,
  filter application).
- Manual: `cargo run` inside a Wayland session.

## Evolution After v0.2.0

Following the initial 0.2.0 release:
1. **Workspace Modularization (v0.2.2)**: `arca-core` was formally split into `crates/arca-xdg` (pure-std Freedesktop specifications: Base Dirs, User Dirs, RFC 8089 URIs, MIME sniffing, `.desktop`, and FreeDesktop Trash v1.0) and `crates/arca-engine` (application state machine, navigation, file operations, configuration, thumbnail caching).
2. **Viewport Virtualization**: All directory listing views (**Grid**, **List**, and **Miller columns**) were virtualized to guarantee execution within Lens's 1 MiB immediate-mode frame arena budget (`tests::large_directory_does_not_overflow_frame_arena`).
3. **Motion Dynamics & Optics v0.0.60 Alignment (ADR-0004)**: Adopted Optics `transit` for centralized cubic easing and spring dynamics in Quick Look modal transitions, and hardware-accelerated premultiplied thumbnail uploads via `flux_image_update_region_premultiply`.
4. **Regional Damage & Quiescence (Optics v0.0.74, ADR-0008)**: Adopted the bounded regional damage rect list, departure damage for removed rows, host-controlled caret/selection placement, and zero-damage idle presentation.
5. **Native Frame Damage & Caret Phases (Optics v0.0.76, ADR-0009)**: Delegated the per-image damage epoch ledger to Iris/Flux, moved focus to build-site label resolution, disabled the caret blink under reduced motion, and verified scrolling viewport damage and caret-flip-only repaints.
6. **Record-Only Host Canvas (Optics v0.0.78, ADR-0010)**: Retired the deleted `PaintHost`/live-paint contract, kept both entry points on the pure-chrome record seam with an empty `RecordHost`, and audited the Flux snapshot, dynamic-slot, and DisplayList wire contracts against Arca's write-once thumbnail ownership.
7. **Unified Bindings & Pure-Rust Motion (Optics v0.0.79, ADR-0011)**: Moved the local patch to the unified `bindings/crates/` workspace, retired the `transit-sys` FFI crate and its rpath relay, and kept icon registration on Lens so every shipped SVG compiles through the shared `reticle` parser without an application-side duplicate.
