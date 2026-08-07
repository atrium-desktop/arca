# AGENTS.md

Guidance for AI agents and contributors working in this repository.

## What this is

Lantern is a file manager for Wayland. It is a Cargo workspace built on the
**optics** stack living in the sibling checkout `../optics`
(flux / lens / iris C libraries plus their official Rust bindings).

## Layout

- `crates/lantern-core` — all business logic. **No GUI dependencies.**
  Directory model, history, bookmarks, sort/filter, file ops, trash, config.
- `crates/lantern-ui` — the only crate that talks to `iris` / `lens` /
  `lens-sys`. Renders `AppState` per frame; owns text buffers, focus and
  popup state.
- `crates/lantern` — thin binary (`main.rs` only) plus the rpath-relay
  `build.rs`.
- `docs/` — user/dev docs; `docs/dev/documentation/` is a governance policy
  directory: do not modify it (human-maintained).

## Build & test

```bash
cargo build        # needs ../optics built once (meson setup build && meson compile -C build)
cargo test         # core unit tests + headless lens frame tests; no GPU needed
cargo run          # needs a Wayland session
```

Never add `LD_LIBRARY_PATH` workarounds: runtime library lookup is handled
by rpath-relay `build.rs` files reading `DEP_IRIS_RS_RPATHS` (mirrors the
pattern used by the optics bindings themselves).

## Conventions

- Keep the dependency tree at zero third-party crates (std only) on the
  `lantern-*` side; the optics bindings are the only path dependencies.
  Discuss before adding any crates.io dependency.
- Logic goes down into `lantern-core` (unit-test it there); `lantern-ui`
  stays a thin rendering/event layer.
- The safe `lens` API is preferred. When a needed symbol is not wrapped
  (full icon set, focus-by-id, clipboard), call `lens_sys` through
  `frame.as_raw()` inside small helpers in `lantern-ui` (see
  `src/icons.rs`), never ad-hoc at call sites.
- File-manager strings are English; code comments explain *why*, not *what*.
- `Esc` quitting the app is iris-level behaviour — do not reimplement it.
