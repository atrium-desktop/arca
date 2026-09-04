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
  `../optics/bindings/{iris,lens,lens-sys}`. `iris-sys`'s `build.rs` walks
  up from its manifest, finds the optics checkout (presence of
  `libs/iris/include` + `build/meson-uninstalled`), and links the meson
  build tree through the `*-uninstalled.pc` pkg-config files — no
  `meson install` required.
- The `iris` crate re-publishes the resulting library directories as
  `links = "iris_rs"` rpath metadata. Both `arca-ui` and `arca` carry
  a small `build.rs` relaying `DEP_IRIS_RS_RPATHS` into
  `-Wl,-rpath` arguments (with `--disable-new-dtags` so transitive deps
  resolve), which lets binaries and test executables run without
  `LD_LIBRARY_PATH`.
- Where the safe `lens` surface is intentionally narrow, `arca-ui`
  drops to `lens_sys` via `frame.as_raw()` inside dedicated helpers:
  the full feather icon set (`icons.rs`), focusing a widget by id
  (Ctrl+L / Ctrl+F / inline rename), and clipboard copy (`menus.rs`).
  Widget ids are captured from `lens_get_response().id` right after the
  widget is built, so no id-stack assumptions are made.

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

## Known limitations

- F-keys are not mapped by lens input, so rename lives on `Ctrl+E`.
- `DeletionDate` in `.trashinfo` is written in UTC (no timezone database
  in a zero-dependency build).
- The file list is not virtualized; extremely large directories render
  every row (fine for daily use; revisit with `lens_table` if needed,
  trading away per-row icons).
