---
id: ADR-0013
title: "Synchronize with Optics v0.0.82, Semantic Module Naming, and the Zero-Stutter Namespace"
status: accepted
date: 2026-11-20
scope: [build/dependencies, ui/arca-ui, dev/standards]
superseded_by: null
negative_knowledge: true
---

# 0013. Synchronize with Optics v0.0.82, Semantic Module Naming, and the Zero-Stutter Namespace

- Status: Accepted
- Date: 2026-11-20
- Deciders: Arca Architecture Working Group
- Consulted: Optics App/Window/UI/Gfx/Motion Maintainers
- Amends: [ADR-0012](0012-synchronize-with-optics-v0.0.80-and-aperture-shell.md)
- Extends: Optics ADR-0169 (root `crates/` layout and shared build helper), ADR-0170 (semantic module naming and `optics_` namespace), ADR-0172 (zero-stutter API and window/app boundary)

---

## Context and Problem Statement

Optics `v0.0.82` renames every library, header, symbol, crate, and pkg-config
file to its canonical semantic name (Optics ADR-0169, ADR-0170) and removes the
stutter the mechanical prefixing introduced (Optics ADR-0172):

1. **Module names replace codenames**: `flux` → `gfx`, `glyph` → `text`,
   `reticle` → `svg`, `vista` → `scene`, `prism` → `material`, `transit` →
   `motion`, `iris` → `window`, `lens` → `ui`, `aperture` → `app`. C symbols
   become `optics_<module>_*`, headers live under `<optics/...>`, libraries are
   `liboptics_<module>`, and pkg-config packages are `optics-<module>`.

2. **Crates moved to the repository root**: the binding crates now live in the
   optics root Cargo workspace under `crates/optics-<module>` and
   `crates/optics-<module>-sys` with a shared `optics-build` helper (Optics
   ADR-0169). The `bindings/crates/` tree is gone, so every local `[patch]`
   path in Arca was dangling.

3. **Zero stutter and a clean window/app seam (Optics ADR-0172)**: the C
   surface lost its `optics_<mod>_<mod>_*` duplicates, `libs/window` now owns a
   pure window/loop vocabulary, and `libs/app` gained the application runtime
   controls. The Rust bindings mirror this: `optics_ui::*` widgets,
   `optics_window::*` platform control, `optics_gfx_sys::*` images,
   `optics_app::*` lifecycle.

4. **New `links` keys**: `optics-app`, `optics-window`, and `optics-ui`
   republish rpaths as `optics_app_rs`, `optics_window_rs`, and `optics_ui_rs`.
   The old `aperture_rs` / `iris_rs` metadata no longer exists, so the rpath
   relay silently produced no `-Wl,-rpath` arguments and every test binary
   required `LD_LIBRARY_PATH`.

Arca was pinned to `v0.0.80`, declared the retired crate names, and called
`lens_*`, `iris_*`, and `flux_*` symbols that no longer exist.

---

## Decision Drivers

- Driver 1: **One Name per Module**: Arca code, documentation, and packaging
  must use the semantic module name, never the retired codename.
- Driver 2: **Layer Visibility**: Widgets from `optics-ui`, platform control
  from `optics-window`, lifecycle from `optics-app`, and image FFI from
  `optics-gfx-sys` stay named at each call site.
- Driver 3: **Canonical/Local Identity Parity**: the tagged `Cargo.toml` and
  the local `[patch]` template must expose the same crate set and the same
  repository-root paths.
- Driver 4: **Mechanical Enforcement**: the headless-engine firewall and the
  documentation gates must reject the retired names instead of relying on
  review.

---

## Architectural Decisions

### 1. Workspace Dependencies Track the Semantic Crates

`Cargo.toml` declares `optics-app`, `optics-window`, `optics-ui`,
`optics-ui-sys`, `optics-gfx-sys`, and `optics-motion` at tag `v0.0.82`.
`optics-app` and `optics-ui` still pull the safe `optics-gfx` crate
transitively, so it is not declared directly.

### 2. Imports Name the Owning Module

- `lens::{Frame, Input, Color, ...}` and `lens::patterns::*` become
  `optics_ui::*`.
- `lens_sys::lens_*` becomes `optics_ui_sys::optics_ui_*` in the narrow
  helper modules (`icons.rs`, `content.rs`, `app.rs`, `preview.rs`,
  `chooser.rs`).
- `iris::*` becomes `optics_window::*`.
- `aperture::{Application, Config, RunError, StartHost, StopHost, RecordHost}`
  becomes `optics_app::*`; `StartHost::flux_device()` becomes
  `StartHost::optics_gfx_device()`.
- `transit::{Spring, SpringParams}` becomes `optics_motion::*`.
- `flux_sys::flux_*` becomes `optics_gfx_sys::optics_gfx_*`.

The icon adapter's public name follows the vocabulary: `icons::lens_id` is now
`icons::icon_id`, and the invalid-id sentinel is
`optics_ui_icon_id::OPTICS_UI_ICON_COUNT`.

### 3. Native Rpath Relay Follows the New `links` Keys

`arca-ui`'s build script relays
`DEP_OPTICS_APP_RS_RPATHS`, `DEP_OPTICS_WINDOW_RS_RPATHS`, and
`DEP_OPTICS_UI_RS_RPATHS`; `arca`'s relays `DEP_OPTICS_APP_RS_RPATHS`. The
full-version pre-1.0 SONAMEs remain the load-time guard against a stale system
library. `optics-motion` stays pure Safe Rust (Optics ADR-0158) and has no
relay.

### 4. Local Patches Track the Root `crates/` Tree

`.cargo/optics-local.toml` (and the generated `.cargo/config.toml`) patch
`../optics/crates/optics-{app,app-sys,build,gfx,gfx-sys,ui,ui-sys,window,window-sys,motion}`.
`optics-build` is patched because the sibling crates resolve it as a path
dependency inside the Optics workspace.

### 5. Firewalls and Documentation Adopt the New Vocabulary

The `[INV-ARCH-02]` CI gate now rejects `optics-app*`, `optics-ui*`,
`optics-window*`, `optics-gfx*`, `optics-text*`, `optics-material*`,
`optics-svg*`, `optics-scene*`, `optics-motion`, and `optics`. Contributor,
packaging, setup, testing, acceptance, how-to, and glossary pages name the
semantic modules; `docs/dev/optics-migration.md` keeps the older codenames
only in its historical evolution log.

---

## Consequences

### Positive

- Arca compiles and runs against the tagged Optics `v0.0.82` release with no
  codename aliases and no local compatibility shim.
- The headless-engine firewall, packaging requirements, and `pkg-config`
  probes all name the same modules the runtime libraries export.
- The rpath relay derives from the current `links` keys, so local builds and
  test binaries again resolve the uninstalled meson tree without
  `LD_LIBRARY_PATH`.

### Negative & Trade-offs

- Requires Optics `v0.0.82` or later. The renamed SONAMEs make `v0.0.80`/`v0.0.81`
  libraries fail at load time rather than misread a descriptor.
- Every documentation page that named a codename had to be rewritten in the
  same change; historical pages still describe the old names, which is
  deliberate.
- The workspace-local `Cargo.lock` remains worktree-only; the canonical
  lockfile must be regenerated against the tag in the primary `main` worktree.

---

## Rejected Alternatives & Negative Knowledge

### 1. Keep local aliases (`use optics_ui as lens;`) to avoid touching call sites

- **Why considered**: a mechanical `use` rename would have been a much
  smaller diff.
- **Why rejected**: the aliases would hide which library owns a type and
  would silently keep the retired vocabulary alive in every review. The
  rename is the point of Optics ADR-0170.

### 2. Re-export `optics_gfx_sys` through `optics_ui_sys` for image calls

- **Why considered**: one `use` for all FFI.
- **Why rejected**: Optics already re-exports the opaque handles; the image
  creation and upload functions belong to the graphics module. Naming the
  owning crate keeps the dependency direction visible.

### 3. Keep the `aperture_rs`/`iris_rs` rpath relay and add the new keys

- **Why considered**: backwards compatibility with an older Optics checkout.
- **Why rejected**: `Cargo.toml` pins one tag. Reading metadata that only
  older Optics publishes would silently produce an empty relay on the pinned
  release, which is exactly the failure this change fixes.

### 4. Route window control through `optics_app_sys::optics_app_*`

- **Why considered**: Optics ADR-0172 adds application-level runtime controls
  (`optics_app_quit`, `optics_app_request_frame`, `optics_app_post`,
  `optics_app_get_geometry`) that delegate to the active window loop.
- **Why rejected**: `optics-app`'s safe surface exposes only `Application`,
  `Config`, and the hosts; the runtime controls are raw `optics-app-sys` FFI.
  Arca's call sites (`window_close`, `request_animation_frame`,
  `post_to_main_thread`) are window-level control, keep their safe wrappers,
  and would otherwise need a second FFI dependency plus `unsafe` blocks for a
  verbatim delegation. The mapping is revisited if the safe crate wraps them.

---

## Links

- [ADR 0010: Synchronize with Optics v0.0.78](0010-synchronize-with-optics-v0.0.78-and-record-only-host-canvas.md)
- [ADR 0011: Synchronize with Optics v0.0.79](0011-synchronize-with-optics-v0.0.79-and-unified-bindings.md)
- [ADR 0012: Synchronize with Optics v0.0.80](0012-synchronize-with-optics-v0.0.80-and-aperture-shell.md)
- [Optics Migration History](../dev/optics-migration.md)
- [Local Optics Development](../dev/optics-local-development.md)
- Optics ADR-0169: Root `crates/` Layout and Shared Build Helper
- Optics ADR-0170: Semantic Module Naming and the `optics_` Namespace
- Optics ADR-0172: Zero-Stutter Public API and Clean window vs. app Runtime Boundary
