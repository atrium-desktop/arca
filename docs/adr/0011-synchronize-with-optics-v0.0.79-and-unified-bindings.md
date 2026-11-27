---
id: ADR-0011
title: "Synchronize with Optics v0.0.79, Unified Bindings, and Pure-Rust Motion"
status: accepted
date: 2026-11-12
scope: [build/dependencies, ui/arca-ui, dev/standards]
superseded_by: null
negative_knowledge: true
---

# 0011. Synchronize with Optics v0.0.79, Unified Bindings, and Pure-Rust Motion

- Status: Accepted
- Date: 2026-11-12
- Deciders: Arca Architecture Working Group
- Consulted: Optics/Lens/Iris Maintainers
- Amends: [ADR-0010](0010-synchronize-with-optics-v0.0.78-and-record-only-host-canvas.md)
- Amended by: [ADR-0025](0025-synchronize-with-optics-v0.0.88-and-app-owned-icon-decoding.md) (Optics v0.0.88 retired the shared SVG compiler; Arca decodes its icons and registers them per `optics_ui` context)
- Extends: Optics ADR-0158 (header-only transit), ADR-0159 (unified bindings workspace and `optics` facade), ADR-0160 (C umbrella facade), ADR-0161 (reticle compiler and vista boundary)

---

## Context and Problem Statement

Optics `v0.0.79` changes how downstream projects consume the stack rather
than changing the Lens/Iris/Flux Rust APIs Arca calls:

1. **One bindings workspace (Optics ADR-0159)**:
   The per-module Cargo trees (`bindings/iris-rs`, `bindings/lens-rs`,
   `bindings/flux-rs`, `bindings/transit-rs`, ...) are deleted. Every Rust
   crate now lives under `bindings/crates/` in one workspace and resolves to
   a single lockstep version. A new optional `optics` facade re-exports the
   layers as `optics::ui`, `optics::platform`, `optics::canvas`, and
   `optics::motion`.

2. **Transit is no longer a shared library (Optics ADR-0158)**:
   `libtransit.so`, `transit.pc`, and the `transit-sys` crate are retired.
   The motion vocabulary is header-only C and pure Safe Rust. A Rust host
   compiles springs and easing directly into its own crate.

3. **SVG parsing moved from `svg` to `reticle` (Optics ADR-0161)**:
   The vector-document compiler is renamed across headers, symbols, and
   Rust crates. Lens no longer carries a private tag scanner:
   `lens_icon_register_svg` now parses and shapes through `reticle_parse`
   and `reticle_shape_at`, with the same public Lens entry point.

4. **Distribution surfaces were unified but remain optional (Optics
   ADR-0160)**:
   `<optics/optics.h>` and `optics.pc` give C consumers one umbrella entry
   point while the granular headers and libraries remain first-class. Vista
   scene-graph tests moved out of `tests/flux`; no host API changed.

Arca was pinned to `v0.0.78`, still listed `transit-sys`, and its local
patch template pointed at binding directories that no longer exist in
`v0.0.79`. The retired sys crate existed only to relay a native rpath, so
keeping it would either fail to resolve or reintroduce a dependency on a
library the new release does not build.

---

## Decision Drivers

- Driver 1: **Reproducible Dependency Surface**: Every workspace dependency
  must resolve from the `v0.0.79` tag, with no reference to deleted crates.
- Driver 2: **No Dead FFI**: Motion math must compile into Arca instead of
  crossing a shared-library boundary that no longer exists.
- Driver 3: **One Icon Pipeline**: Arca's SVG assets must be parsed by the
  same compiler Lens uses, not a second in-tree or host-side parser.
- Driver 4: **Local Worktree Fidelity**: `.cargo/optics-local.toml` must patch
  the unified `bindings/crates/` layout so daily development tracks the
  same crate identities as the tagged build.
- Driver 5: **No Speculative Adoption**: Optional distribution facades are
  adopted only when they remove a real Arca responsibility.

---

## Architectural Decisions

### 1. Optics Dependency Floor Bump

All workspace dependencies for `iris`, `lens`, `lens-sys`, `flux`,
`flux-sys`, and `transit` are bumped to `v0.0.79`. `transit-sys` is removed
from the workspace and from both crates that named it. The canonical
`Cargo.lock` records the tagged commit; local development resolves the
sibling checkout through `.cargo/optics-local.toml`.

### 2. Local Patches Follow the Unified Workspace

`.cargo/optics-local.toml` (and the ignored `.cargo/config.toml` copy)
patch `../optics/bindings/crates/{flux,flux-sys,lens,lens-sys,iris,iris-sys,transit}`.
The `bindings/*-rs` paths are not kept as aliases: a stale patch path would
silently fall back to the remote tag or fail resolution, which is exactly
the local-illusion failure the worktree workflow exists to prevent.

### 3. Pure-Rust Motion Replaces the Transit Rpath Relay

`arca-ui` keeps its direct `transit` dependency and its `Spring` /
`SpringParams` usage. The direct `transit-sys` dependencies in `arca-ui` and
`arca` are deleted together with the `DEP_TRANSIT_*_RPATHS` handling in both
`build.rs` files. Only the Iris rpath relay remains, because Iris still owns
native libraries and publishes `DEP_IRIS_RS_RPATHS`.

No compatibility shim or local `libtransit.so` is loaded: the v0.0.79 build
tree produces no motion shared object, and a leftover v0.0.78 artifact is
never part of the dependency graph.

### 4. Icons Stay on the Lens Registration Seam

Arca continues to embed its 35 SVG assets and call
`lens::register_svg_icon` exactly once per glyph through the process-global
`REGISTERED` table. In `v0.0.79` that C entry point compiles through the
shared `reticle` parser, so Arca inherits line-command fidelity and the
removal of Lens's duplicated tag scanner without adding a direct `reticle`
or `reticle-sys` dependency. The existing
`all_icon_assets_register` test fails if any shipped glyph stops parsing.

Arca does not rasterize SVG documents itself. If it ever needs to (for
example a custom preview surface), it must depend on `reticle` directly
rather than duplicating parsing in the application.

### 5. The Optional Distribution Facades Are Not Adopted

- The Rust `optics` facade is not added. Arca consumes four explicit
  layers (`iris`, `lens`, `flux-sys`, `transit`), and both the facade and
  the granular crates resolve to the same `v0.0.79` workspace. Renaming
  every import to `optics::ui` / `optics::platform` would add a second
  naming layer without changing the linked code; the facade remains an
  option for consumers that prefer one dependency line.
- The C `<optics/optics.h>` / `optics.pc` umbrella is not adopted because
  Arca has no C translation units and links native libraries only through
  the `*-sys` crates.
- The Vista boundary is not relevant: Arca renders no glTF scene graph and
  calls no Vista API.

---

## Consequences

### Positive

- The workspace resolves entirely against real `v0.0.79` crate names; no
  deleted `transit-sys` or `bindings/*-rs` path remains.
- Motion math is inlined into Arca's frame path instead of dispatched
  through a PLT entry, and packaging no longer expects `libtransit.so`.
- All shipped icons are parsed by the same reticle compiler as upstream
  Lens icons, with one registration path and one failure test.
- Local and canonical builds now select the same binding identities; only
  the source location differs.

### Negative & Trade-offs

- Requires Optics `v0.0.79` or later. Installed native libraries from older
  releases fail at load time through the full-version SONAME for
  flux/lens/iris; there is no compatibility layer.
- Arca intentionally opts out of the new facade naming. A future migration
  to `optics::*` imports is a source-only change and does not affect the
  crate graph, but it remains deliberately deferred.

---

## Rejected Alternatives & Negative Knowledge

### 1. Stay on `v0.0.78` until a larger feature requires the bump

- **Why considered**: avoids touching manifests and build scripts.
- **Why rejected**: the local patch paths already point into deleted
  directories, and the tagged `transit-sys` crate no longer corresponds to
  a library built by the current Optics tree. The pin would preserve a
  dependency on a packaging surface upstream explicitly retired.

### 2. Keep `transit-sys` as a local compatibility shim

- **Why considered**: preserves the old rpath relay and lockfile shape.
- **Why rejected**: there is no `libtransit.so` in `v0.0.79`; a shim would
  either link a stale artifact or expose a fake links metadata crate. Both
  hide the real change and reintroduce FFI overhead for pure arithmetic.

### 3. Add `reticle` / `reticle-sys` for icon parsing

- **Why considered**: makes Arca's SVG dependency explicit.
- **Why rejected**: Lens owns icon registration, theme tinting, atlas
  storage, and damage. Parsing documents a second time in the application
  would create a parallel icon path with no rendered benefit.

### 4. Adopt the `optics` facade crate wholesale

- **Why considered**: one dependency line and upstream's canonical naming.
- **Why rejected**: it changes import spelling, not behavior or version
  alignment. Arca's layer usage is already explicit and auditable, and the
  umbrella facade is documented upstream as optional convenience rather
  than a requirement.

### 5. Link stale native `libtransit.so.0.0.78` from the build tree

- **Why considered**: the old Meson build directory may still contain it.
- **Why rejected**: `v0.0.79` does not build or install that library.
  Depending on a leftover file would make a clean checkout fail and turn
  local build residue into an accidental runtime contract.

---

## Links

- [ADR 0001: Optics Stack Migration](0001-optics-stack-migration.md)
- [ADR 0008: Synchronize with Optics v0.0.74](0008-synchronize-with-optics-v0.0.74-and-damage-quiescence.md)
- [ADR 0009: Synchronize with Optics v0.0.76](0009-synchronize-with-optics-v0.0.76-and-native-frame-damage.md)
- [ADR 0010: Synchronize with Optics v0.0.78](0010-synchronize-with-optics-v0.0.78-and-record-only-host-canvas.md)
- [Optics Migration History](../dev/optics-migration.md)
- Optics ADR-0158: Transit Header-Only Vocabulary
- Optics ADR-0159: Unified Cargo Workspace and Optics Facade Bindings
- Optics ADR-0160: Unified Optics C Facade and Umbrella Distribution
- Optics ADR-0161: Reticle Vector Compiler and Vista Boundary
