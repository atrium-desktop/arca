# AGENTS.md

Guidance for AI agents and contributors working in this repository.

## What this is

Arca is a file manager for Wayland, and a companion application for the
**tessera** desktop: its visual language follows the tessera design system
(`../tessera-dev/docs/dev/design`, application tokens in
`../tessera-dev/crates/tessera-design`) while remaining a standalone Wayland
client. It is a Cargo workspace built on the
**optics** stack (flux / lens / iris C libraries plus their official Rust
bindings). The canonical build resolves the bindings from the tagged optics
monorepo and links the system-installed C libraries; the linked
`../arca-dev` worktree patches the bindings to the sibling `../optics`
checkout for cross-repository development.

## Layout

- `crates/arca-xdg` — pure-Rust, zero-dependency Freedesktop.org (XDG)
  specifications: Base Directories, User Directories, RFC 8089 File URIs,
  Shared MIME-info with content sniffing, Desktop Entry & mimeapps.list,
  FreeDesktop Trash v1.0, and standard clipboard protocols.
- `crates/arca-engine` — file manager engine and session state. **No GUI dependencies.**
  Directory model, history, bookmarks, sort/filter, file ops, config,
  thumbnail service conforming to FreeDesktop standard.
- `crates/arca-ui` — the only crate that talks to `iris` / `lens` /
  `lens-sys`. Renders `AppState` per frame; owns text buffers, focus, popup
  state, the GPU thumbnail store (`src/thumbs.rs`) and the icon runtime
  (`src/icons.rs`).
- `assets/icons/` — Arca's own icon set: plain SVGs, registered with
  lens at runtime and drawn through its native icon widgets. Single source
  of truth, no generation step.
- `crates/arca` — thin binary (`main.rs` only) plus the rpath-relay
  `build.rs`.
- `docs/` — user, dev, and governance documentation;
  `docs/governance/documentation/` is a governance standard directory managed
  via `docs-governance` protocol v3.1.0: do not modify standard files
  directly; follow the sync/verify toolchain.

## Build & test

```bash
cargo build        # canonical mode: needs the matching optics release installed (pkg-config finds iris/lens/flux)
cargo test         # core unit tests + headless lens frame tests; no GPU needed
cargo run          # needs a Wayland session
```

Cross-repository work happens in the `../arca-dev` worktree (local `dev`
branch), where `.cargo/config.toml` patches the bindings to `../optics`;
build the sibling meson tree first (`meson compile -C ../optics/build`). The
full workflow is in `docs/dev/optics-dev-worktree.md`. Local patch
state (`Cargo.lock`, `.cargo/config.toml`) never enters commits — the
pre-commit hook (`git config core.hooksPath .githooks`) unstages it.

Arca relies on optics APIs added after v0.0.10 —
`iris::Application::run_with_start` (device handover for texture uploads),
`lens_icon_register_svg` (runtime icons) and `lens_scroll_offset` — so a
canonical build needs an optics tag containing them (v0.0.11+).

Never add `LD_LIBRARY_PATH` workarounds: runtime library lookup is handled
by rpath-relay `build.rs` files reading `DEP_IRIS_RS_RPATHS` (mirrors the
pattern used by the optics bindings themselves).

## Conventions

- The `arca-*` crates stay std-only except for one approved crates.io
  dependency: the `image` crate (features `png`, `jpeg` only) in
  `arca-engine`, which decodes thumbnails — the optics stack has no image
  decoder. Discuss before adding any other dependency or enabling more
  `image` formats.
- Logic goes down into `arca-engine` (unit-test it there); `arca-ui`
  stays a thin rendering/event layer.
- The safe `lens` API is preferred. When a needed symbol is not wrapped
  (focus-by-id, clipboard, image upload/draw), call `lens_sys` through
  `frame.as_raw()` inside small helpers in `arca-ui` (see
  `src/icons.rs`, `src/thumbs.rs`, `src/device.rs`), never ad-hoc at call
  sites.
- Icons come from Arca's own asset set (`assets/icons/*.svg`),
  registered at runtime via `lens_icon_register_svg` and drawn with the
  native lens icon widgets. Keep glyphs 24×24, single-colour, stroke or
  fill (nonzero) — runtime icons share the built-ins' conventions.
- Large listings are virtualized: grid/list/Miller only build the visible
  row window (lens's 1 MiB per-frame arena drops draw calls silently past
  the limit — blank icons). Keep per-frame widget counts bounded; the
  `large_directory_does_not_overflow_frame_arena` test guards this.
- File-manager strings are English; code comments explain *why*, not *what*.
- `Esc` quitting the app is iris-level behaviour — do not reimplement it.
