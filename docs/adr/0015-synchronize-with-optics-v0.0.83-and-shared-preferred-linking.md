---
id: ADR-0015
title: "Synchronize with Optics v0.0.83, Shared-Preferred Linking, and the Additive Static Feature"
status: accepted
date: 2026-11-23
scope: [build/dependencies, build/link-mode, packaging/distribution]
superseded_by: null
negative_knowledge: true
---

# 0015. Synchronize with Optics v0.0.83, Shared-Preferred Linking, and the Additive Static Feature

- Status: Accepted
- Date: 2026-11-23
- Deciders: Arca Architecture Working Group
- Consulted: Optics Build/Packaging Maintainers, Distribution Packers
- Amends: [ADR-0014](0014-optics-link-mode-and-static-release.md)
- Extends: [ADR-0013](0013-synchronize-with-optics-v0.0.82-semantic-module-naming.md)
- References: Optics ADR-0169 / ADR-0170 / ADR-0172 / ADR-0173

---

## Context and Problem Statement

Optics `v0.0.83` is a breaking Rust-consumer release even though the C public
surface Arca calls is unchanged:

1. **The default link mode is now shared-preferred.** With no feature and no
   environment variable, every `optics-*-sys` crate links `liboptics_*.so`
   when it exists and uses the archives only on a static-only tree. `v0.0.82`
   embedded the archives whenever they existed. A development build that used
   to be self-contained now needs the shared objects at runtime (or an rpath).

2. **Every `-sys` crate, wrapper crate, and the `optics` facade gained an
   additive `static` Cargo feature.** It forwards down the graph and fails when
   an archive or a private dependency is missing instead of degrading to a
   shared link. There is deliberately no `dynamic` feature: Cargo features are
   additive and unified across the graph. `OPTICS_STATIC` / `OPTICS_DYNAMIC`
   remain top-priority environment overrides, so a packager can still force
   either mode over the feature.

3. **Link-mode selection was hardened in `optics-build`.** Static-only trees
   now link, a probe failure is reported instead of silently dropping
   `Requires.private`, contradictory switches are a hard error,
   `PKG_CONFIG_ALL_STATIC` / `PKG_CONFIG_ALL_DYNAMIC` are honoured as aliases,
   `OPTICS_DYNAMIC=1` verifies that a shared object exists, and one link mode
   is enforced for the whole build. The public Rust names finished the semantic
   rename: `optics_app::iris` is now `optics_app::window`, and
   `optics_ui::FluxDiagnostic` is now `GfxDiagnostic`.

4. **Retired scaffolding (Optics ADR-0173).** `vault/earcut`, the shader
   fallback, the `OPTICS_GFX_INIT` upper-case aliases, and their Rust
   counterparts are gone; a C23 compiler with `#embed` is required. Arca never
   used those entry points.

Arca was pinned to `v0.0.82`, whose ADR-0014 describes a static-preferred
default. Leaving the pin alone would build against sources whose default link
mode no longer matches the documented contract, and the local worktree
documentation would keep claiming that the archives are embedded while the
default now loads `liboptics_*.so`.

---

## Decision Drivers

- Driver 1: **One link contract**: Arca documents and verifies the actual
  default of the pinned Optics release, never a stale one.
- Driver 2: **Build-input flexibility**: `OPTICS_STATIC=1`, `OPTICS_DYNAMIC=1`,
  the `static` feature, and the shared-preferred default must all stay
  available (ADR-0014).
- Driver 3: **Reproducible release**: the shipped binary must still embed the
  Optics modules regardless of what the release host happens to provide.
- Driver 4: **Canonical/local parity**: the tagged dependencies and the local
  `[patch]` template must resolve the same crate set and version.
- Driver 5: **No needless source churn**: adopt upstream renames only where
  Arca actually names the changed item.

---

## Architectural Decisions

### 1. Repin the Workspace to `v0.0.83`

`Cargo.toml` and `Cargo.lock` track `optics-app`, `optics-window`,
`optics-ui`, `optics-ui-sys`, `optics-gfx-sys`, and `optics-motion` at tag
`v0.0.83`. The crate set and the repository-root `crates/optics-<module>`
layout are unchanged, so `.cargo/optics-local.toml` needs no new patch entries.

### 2. Keep the Link Mode a Build Input

Arca does **not** enable the `static` feature in its manifest. Enabling it
would be a repository-level rule that removes the shared mode from every
downstream packager, which ADR-0014 explicitly rejects. The manifest stays
mode-neutral; the release invocation pins `OPTICS_STATIC=1`, and
`scripts/check-optics-link.sh --expect static` verifies the ELF output. The
`static` feature remains available to any consumer that wants to opt in.

### 3. The Development Default Is Now Shared

The canonical and local development builds now link `liboptics_*.so` by
default. This is a documentation and diagnostics change, not a build-script
change: `crates/arca/build.rs` and `crates/arca-ui/build.rs` already relay the
`DEP_*_RPATHS` metadata that the shared branch publishes, so binaries and
headless tests find the uninstalled tree without `LD_LIBRARY_PATH`. A developer
who wants the former embedded-archive behaviour exports `OPTICS_STATIC=1` (or
enables `static`). `scripts/check-optics-link.sh target/debug/arca` now reports
`dynamic` on the sibling Meson tree, which is the expected result rather than a
misconfiguration.

### 4. Adopt the Rust Re-export Renames Where Used

`optics_app::iris` became `optics_app::window`, and
`optics_ui::FluxDiagnostic` became `GfxDiagnostic`. Arca imports
`optics_ui::*`, `optics_window::*`, and `optics_app::{Application, Config,
RunError, StartHost, StopHost, RecordHost}` directly and does not name either
retired item, so no `crates/**/*.rs` call site changed. The renames are recorded
so a future import does not silently reach for the old path.

### 5. Documentation and Gates Follow the Same Release

`docs/dev/build-and-run.md`, `setup.md`, `testing.md`, `packaging.md`,
`optics-dev-worktree.md`, `optics-migration.md`, `acceptance.md`,
`how-to/install-and-package.md`, the glossary, README, and the derived rpath
comments describe the shared-preferred default, the `static` feature, and the
`OPTICS_STATIC=1` release pin. `[INV-BUILD-01]` is unchanged: it constrains the
*release* artifact, not every development build.

---

## Consequences

### Positive

- Arca compiles against the tagged `v0.0.83` sources and the local worktree
  records the same version.
- The development default matches the upstream `-sys` convention: the shared
  objects are used when present, and relinking after a C change is avoided.
- The release artifact keeps its self-contained contract: no
  `liboptics_*.so` `DT_NEEDED` entry and no build-tree runpath.
- The hardened upstream selector removes the mixed-mode and dropped-private-
  dependency failure classes that could previously produce a broken binary.

### Negative & Trade-offs

- A default `cargo run` / `cargo test` now needs the shared objects at runtime.
  The rpath relay covers the local and CI trees; an installed prefix must be on
  the loader path or the build must opt into `static`/`OPTICS_STATIC=1`.
- `scripts/check-optics-link.sh` reports `dynamic` for development artifacts,
  so release notes must quote the `--expect static` release result, not a
  development build's mode.
- The pinned C toolchain floor rises: Optics `v0.0.83` requires a C23 compiler
  with `#embed` (GCC >= 15, Clang >= 19). The Arca Rust floor is unchanged at
  1.85.
- A packager that wants both modes must keep `-Ddefault_library=both`; a
  shared-only Optics package can satisfy development but not the release pin.

---

## Rejected Alternatives & Negative Knowledge

### 1. Stay on `v0.0.82`

- **Why considered**: no manifest or documentation change.
- **Why rejected**: the pinned sources would no longer match the documented
  default, and the local worktree would resolve `v0.0.83` sources through the
  path patch while the canonical build resolved `v0.0.82`, splitting the
  tested and shipped code.

### 2. Enable `static` in the Arca manifest

- **Why considered**: makes every Arca build self-contained, including
  development and downstream rebuilds.
- **Why rejected**: Cargo features are unified across the graph, so this would
  remove the shared mode from every packager that depends on Arca's crates or
  reuses its manifest. The environment pin belongs to the release invocation;
  the capability belongs to the caller.

### 3. Add a symmetric `dynamic` feature

- **Why considered**: a feature pair looks more explicit than an environment
  variable.
- **Why rejected**: Cargo features are additive and cannot express mutual
  exclusion. Upstream deliberately ships only `static`; Arca does not invent a
  private feature that would diverge from the pinned bindings.

### 4. Rewrite the build scripts to detect the default

- **Why considered**: a build script could report or enforce the effective
  mode at compile time.
- **Why rejected**: the scripts already react to the published rpaths and must
  not override a legitimate upstream choice. The ELF-level gate observes the
  real artifact, which is stronger than any intent recorded during the build.

---

## Links

- [ADR 0013: Synchronize with Optics v0.0.82](0013-synchronize-with-optics-v0.0.82-semantic-module-naming.md)
- [ADR 0014: Optics Link Mode and Static Release Artifact](0014-optics-link-mode-and-static-release.md)
- [Optics Development Worktree Workflow](../dev/optics-dev-worktree.md)
- [Build & Run Guide](../dev/build-and-run.md)
- [Distribution Packaging](../dev/packaging.md)
