---
id: ADR-0012
title: "Synchronize with Optics v0.0.80, the Aperture Application Shell, and Descriptor-Only Operations"
status: accepted
date: 2026-11-16
scope: [build/dependencies, ui/arca-ui, dev/standards]
superseded_by: null
negative_knowledge: true
---

# 0012. Synchronize with Optics v0.0.80, the Aperture Application Shell, and Descriptor-Only Operations

- Status: Accepted
- Date: 2026-11-16
- Deciders: Arca Architecture Working Group
- Consulted: Optics/Lens/Iris/Aperture Maintainers
- Amends: [ADR-0011](0011-synchronize-with-optics-v0.0.79-and-unified-bindings.md)
- Extends: Optics ADR-0163 (repository-root Cargo workspace), ADR-0165 (single-option descriptors), ADR-0166 (aperture application shell and iris/lens split)

---

## Context and Problem Statement

Optics `v0.0.80` breaks the application-shell boundary and finishes the
descriptor migration that Arca had partially hidden behind safe wrappers:

1. **Aperture owns the application shell (Optics ADR-0166)**:
   Iris no longer depends on Lens. `iris_app_opts` loses `build`, the start
   and stop callbacks lose their `lens *` parameter, and `iris_app_config`
   is gone. A new `aperture` library runs a Lens UI in an Iris window through
   `iris_ui_host`; its Rust crates `aperture` / `aperture-sys` now own
   `Application`, `Config`, `RunError`, `StartHost`, `StopHost`, and
   `RecordHost`. The `iris` crate keeps window, cursor, dialog, and theme
   APIs and no longer re-exports Lens types.

2. **One descriptor per operation (Optics ADR-0165)**:
   The remaining multi-argument operations were migrated to option structs
   across Flux, Glyph, Vista, Prism, Iris, and Lens in six waves. Arca's two
   direct Lens calls (`lens_scroll_offset`, `lens_scroll_to`) and its direct
   Flux image upload must use the new descriptors.

3. **Flux has one creation path (Optics v0.0.80)**:
   `flux_canvas_create_cpu*`, CPU supersampling, MSAA, the no-stencil pass
   switch, and the ear-clipping tessellator are removed. `flux_image_desc`
   gains a required `usage` field; `flux_image_update_region_premultiply`
   is folded into `flux_image_update_region` with a `premultiply` flag.

4. **Workspace root moved (Optics ADR-0163)**:
   The Optics Cargo workspace now lives at the repository root rather than
   under `bindings/`. Crate identities remain `bindings/crates/*`, so local
   patches keep their paths but no longer point at a nested workspace.

Arca was pinned to `v0.0.79` and imported every UI type and widget pattern
through `iris`, which no longer compiles against the new layer graph.

---

## Decision Drivers

- Driver 1: **One Import Boundary per Layer**: Widget and input types come
  from Lens, window controls from Iris, and the application shell from
  Aperture; no application code depends on an accidental re-export.
- Driver 2: **Descriptor Fidelity**: Direct `*-sys` calls must use the same
  single-option structs as the C API so a future descriptor field is a
  compile error rather than a positional-argument mistake.
- Driver 3: **Record-Only Chrome Stays Intact**: Arca remains pure Lens
  chrome; the new aperture shell must keep the empty recorder path and the
  deterministic start/stop lifecycle that owns GPU textures.
- Driver 4: **No Legacy Compatibility Layer**: Removed Flux canvas variants
  and the old Iris application wrapper are not reintroduced locally.
- Driver 5: **Canonical/Local Identity Parity**: The local patch template
  must expose the same crate set, including aperture, as the tagged build.

---

## Architectural Decisions

### 1. Aperture Replaces Iris as the Application Shell

`arca-ui` adds a direct `aperture` dependency. `run` and `run_chooser` build
`aperture::Config`, call `aperture::Application::run_with_lifecycle`, and
pass `aperture::{StartHost, StopHost, RecordHost}`. The start callback still
borrows the Iris-owned Flux device for texture uploads; the stop callback
still releases GPU textures before teardown (ADR-0045).

The recorder remains `None::<fn(aperture::RecordHost)>`. Aperture forwards
that slot to the Iris UI host, so Arca stays pure chrome on both entry
points without depending on the old Iris application wrapper.

### 2. Lens Owns UI Types, Iris Owns the Platform

Every `use iris::{Frame, Input, Color, ...}` and `iris::patterns::*` import
moves to `lens`. Calls that genuinely belong to the platform stay on iris:
`window_close`, `window_start_move`, `window_create_activation_token`,
`request_animation_frame`, `post_to_main_thread`, and
`system_prefers_dark`. Headless tests import keyboard and input types from
lens as well, so the test surface matches production.

No `aperture::*` glob import is introduced: UI modules name `lens` and
`iris` explicitly, and only the application entry points name `aperture`.
This keeps the dependency direction visible at each call site.

### 3. Direct FFI Calls Adopt Their Descriptors

- `content.rs` passes `lens_scroll_offset_opts { id, out_x, out_y }` and
  `lens_scroll_to_opts { id, x, y }` instead of positional arguments.
- `thumbs.rs` initializes `flux_image_desc.usage` to
  `FLUX_IMAGE_USAGE_SAMPLED` and uploads straight RGBA8 through
  `flux_image_update_region_opts { ..., row_bytes: 0, premultiply: true }`.
  The premultiplication still happens in Flux's integer-exact upload path;
  Arca keeps no CPU pixel loop.

The safe `lens::Frame` API already wraps the descriptor migration for widget
operations; no Arca call site changed for segmented controls, tab strips,
data grids, tree views, or popup menus.

### 4. Removed Flux Paths Are Not Recreated

Arca uses no CPU canvas, supersampling, MSAA, stencil switch, tessellator,
render-target image, or compute-write image. The audit is mechanical:
`thumbs.rs` creates sampled images only; `preview.rs` and `content.rs` draw
uploaded images through Lens; neither links a removed canvas entry point.
The full headless suite remains green, including the frame-arena and damage
tests.

### 5. Native Rpath Relay Follows the New Shell

Aperture publishes `DEP_APERTURE_RS_RPATHS`; Iris keeps
`DEP_IRIS_RS_RPATHS`. `arca-ui`'s build script relays the union of both
lists, while the `arca` binary depends directly on aperture and relays its
list. The full-version pre-1.0 SONAMEs remain the load-time guard against a
stale system library. `transit` remains pure Safe Rust (Optics ADR-0158) and
has no native relay.

### 6. Local Patches Track the Root Workspace

`.cargo/optics-local.toml` patches aperture and aperture-sys in addition to
the existing optics crates, all under `../optics/bindings/crates/`. The
patched crates resolve their internal path dependencies inside the sibling
checkout, so local development exercises the same crate graph as the tag.

---

## Consequences

### Positive

- The layer graph matches Optics exactly: aperture → iris/lens → flux, with
  no accidental iris-to-lens import in Arca.
- Direct FFI call sites now fail at compile time when Optics adds a
  descriptor field, instead of silently shifting positional arguments.
- Thumbnail uploads keep integer-exact premultiplication with one descriptor
  and one canonical `flux_image_update_region` entry point.
- Canonical and local builds select the same tagged crate identities; the
  safe `flux` crate is pulled transitively instead of being declared twice.

### Negative & Trade-offs

- Requires Optics `v0.0.80` or later. The full-version SONAMEs make older
  native libraries fail at load time rather than misread a descriptor.
- `arca-ui` now names three Optics layers (aperture, iris, lens) where it
  previously named one (`iris`). This is deliberate: the split is real, and
  hiding it behind a local facade would recreate the coupling upstream
  removed.
- The local lockfile remains worktree-only; the canonical lock must be
  regenerated in the primary `main` worktree after promotion.

---

## Rejected Alternatives & Negative Knowledge

### 1. Keep calling `iris::Application` and add a local compatibility wrapper

- **Why considered**: avoids touching both entry points.
- **Why rejected**: Iris no longer owns build/record callbacks or the Lens
  context. A local wrapper would have to reimplement `iris_ui_host` and
  would drift from Aperture's lifecycle and frame-skip semantics.

### 2. Re-export Lens types from `arca-ui` and keep `iris::*` imports

- **Why considered**: mechanical import compatibility.
- **Why rejected**: it recreates the dependency coupling ADR-0166 removed
  and makes it unclear which library owns a type. Imports now name the
  owning crate directly.

### 3. Retain CPU supersampling or MSAA for thumbnail quality

- **Why considered**: the removed Flux variants promised configurable edge
  quality.
- **Why rejected**: Arca uploads decoded raster thumbnails and draws them
  through Lens. It never rendered vector content into a CPU canvas, so the
  removed creation paths had no call sites and no quality effect.

### 4. Keep `flux_image_update_region_premultiply` as an Arca helper

- **Why considered**: one familiar call.
- **Why rejected**: the C symbol is gone. A helper would only wrap
  `flux_image_update_region_opts { premultiply: true }` and add a second
  name for the same operation.

### 5. Adopt the `optics` facade crate now that it has an `app` feature

- **Why considered**: one dependency line for the whole stack.
- **Why rejected**: Arca needs direct access to aperture, iris, and lens at
  distinct call sites, plus `lens-sys` and `flux-sys` for icons, focus,
  clipboard, and texture uploads. The facade would add a naming layer
  without removing a single direct dependency.

---

## Links

- [ADR 0001: Optics Stack Migration](0001-optics-stack-migration.md)
- [ADR 0010: Synchronize with Optics v0.0.78](0010-synchronize-with-optics-v0.0.78-and-record-only-host-canvas.md)
- [ADR 0011: Synchronize with Optics v0.0.79](0011-synchronize-with-optics-v0.0.79-and-unified-bindings.md)
- [Optics Migration History](../dev/optics-migration.md)
- [Cross-Repository Development Workflow](../dev/cross-repository-development.md)
- Optics ADR-0163: Repository-Root Cargo Workspace
- Optics ADR-0165: Single-Option Descriptor Migration
- Optics ADR-0166: Aperture Application Shell and Iris/Lens Split
