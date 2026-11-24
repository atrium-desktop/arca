---
id: ADR-0016
title: "Default-Static Optics Manifest and Worktree-Local Dynamic Override"
status: accepted
date: 2026-11-24
scope: [build/dependencies, build/link-mode, packaging/distribution]
superseded_by: null
negative_knowledge: true
---

# 0016. Default-Static Optics Manifest and Worktree-Local Dynamic Override

- Status: Accepted
- Date: 2026-11-24
- Deciders: Arca Architecture Working Group
- Consulted: Optics Build/Packaging Maintainers, Distribution Packers
- Amends: [ADR-0014](0014-optics-link-mode-and-static-release.md), [ADR-0015](0015-synchronize-with-optics-v0.0.83-and-shared-preferred-linking.md)
- Amended by: [ADR-0018](0018-canonical-lockfile-and-environment-resolved-optics.md) (replaces the local `[patch]` with environment-resolved discovery)
- References: Optics `v0.0.83` (`optics-build` precedence: `OPTICS_STATIC`/`OPTICS_DYNAMIC` > `static` feature > shared-preferred probe)

---

## Context and Problem Statement

[ADR-0015](0015-synchronize-with-optics-v0.0.83-and-shared-preferred-linking.md)
kept the workspace `Cargo.toml` mode-neutral after repinning to Optics `v0.0.83`,
relying on callers to pass `OPTICS_STATIC=1` for canonical release and packaging
builds while leaving development builds on Optics's shared-preferred default.

In practice, that split created two recurring friction points:

1. **Silent release misbuilds when `OPTICS_STATIC=1` is omitted**: a plain
   `cargo build --release -p arca` on a host with `default_library=both`
   silently produced a dynamically linked binary carrying an Optics SONAME and
   build-tree rpath, failing `[INV-BUILD-01]` only when the ELF gate ran later.
2. **Asymmetric mental model for `main` vs `dev`**: Arca's dual-worktree
   workflow already separates the canonical `main` tree (`../arca/`, no
   `.cargo/config.toml`) from the local cross-repository `dev` tree
   (`../arca-dev/`, untracked `.cargo/config.toml` copied from
   `.cargo/optics-local.toml`). Making `main` require an extra environment
   variable to produce its canonical artifact inverted the principle of least
   surprise.

Meanwhile, Optics `v0.0.83`'s `optics-build` gives `OPTICS_DYNAMIC=1` **strictly
higher precedence** than the additive `static` Cargo feature, and supports
`OPTICS_DYNAMIC=0` to fall back to the manifest's `static` feature when
`OPTICS_DYNAMIC = "1"` is set in `.cargo/config.toml`. Enabling `static` in
Arca's root `Cargo.toml` therefore no longer removes the shared link mode from
developers or downstream packagers.

---

## Decision Drivers

- Driver 1: **Safe-by-default canonical and release builds**: a plain
  `cargo build --release -p arca` in `main`, CI, or distribution recipes must
  embed `liboptics_*.a` without requiring an extra environment variable.
- Driver 2: **Fast zero-relink iteration in `dev`**: cross-repository work in
  `arca-dev` must still link `liboptics_*.so` from `../optics/build` via rpath
  so C and shader edits only need `meson compile -C ../optics/build`.
- Driver 3: **Identical manifests across branches**: `main` and `dev` must
  never diverge on `Cargo.toml` link settings.
- Driver 4: **Preserve build-input flexibility (ADR-0014)**: downstream
  packagers and CI must still be able to select shared linking with
  `OPTICS_DYNAMIC=1`, and `dev` developers must be able to verify static
  closure on demand with `OPTICS_DYNAMIC=0`.

---

## Architectural Decisions

### 1. Enable `features = ["static"]` on Native Optics Dependencies in `Cargo.toml`

The root workspace manifest enables `features = ["static"]` on all native
Optics bindings (`optics-app`, `optics-window`, `optics-ui`, `optics-ui-sys`,
`optics-gfx-sys`; `optics-motion` is pure Safe Rust). Canonical `main` builds,
CI release gates, and distribution packaging recipes therefore embed
`liboptics_*.a` by default without needing `OPTICS_STATIC=1`.

### 2. Override to Dynamic Linking in `.cargo/optics-local.toml`

`.cargo/optics-local.toml` (copied to the git-ignored `.cargo/config.toml` only
inside linked `dev` worktrees) sets:

```toml
[env]
OPTICS_DYNAMIC = "1"
```

Because `optics-build` evaluates environment overrides ahead of Cargo features,
every `cargo check`, `cargo build`, and `cargo test` in `arca-dev` links the
shared objects in `../optics/build` and relays their rpaths through
`crates/arca/build.rs` and `crates/arca-ui/build.rs`. Passing
`OPTICS_DYNAMIC=0` in the shell temporarily disables the override and exercises
the production static closure on demand.

### 3. Keep Both Modes Verified in CI

CI (`.github/workflows/ci.yml`) runs:

1. Plain `cargo build --release -p arca` followed by
   `scripts/check-optics-link.sh --expect static target/release/arca` (before
   and after `strip --strip-debug`), proving `[INV-BUILD-01]` holds by default.
2. `OPTICS_DYNAMIC=1 cargo build --release -p arca` followed by
   `scripts/check-optics-link.sh --expect dynamic target/release/arca`, proving
   the shared-library override and rpath relay remain healthy.

### 4. Reconcile with ADR-0014 and ADR-0015

This decision amends ADR-0015 Decision 2 ("Keep the Link Mode a Build Input" by
omitting `static` from `Cargo.toml`) while preserving ADR-0014's core
invariant: neither mode is removed. Because `OPTICS_DYNAMIC=1` overrides the
`static` feature cleanly in `optics-build`, Arca gains a safe-by-default static
release contract without locking out shared-library consumers or local `dev`
iteration.

---

## Consequences

### Positive

- A plain `cargo build --release -p arca` on `main`, CI, and packaging hosts
  always embeds the Optics native stack (`[INV-BUILD-01]`).
- `main` and `dev` keep byte-identical `Cargo.toml` manifests; all local
  divergences stay confined to the untracked `arca-dev/.cargo/config.toml`.
- Local `dev` iteration keeps fast shared-library linking and rpath discovery,
  while `OPTICS_DYNAMIC=0` allows one-command static verification inside `dev`.

### Negative & Trade-offs

- Contributors building canonical `main` on a shared-only Optics installation
  will see a clear static-archive missing error unless they either install the
  archives (`-Ddefault_library=both`) or explicitly pass `OPTICS_DYNAMIC=1`.
- Downstream Cargo consumers that depend on Arca's crates directly inherit the
  unified `static` feature unless they set `OPTICS_DYNAMIC=1` in the build
  environment (Arca is an end-user application binary, not a published library
  crate, so this trade-off is acceptable).

---

## Rejected Alternatives & Negative Knowledge

### 1. Keep `Cargo.toml` Mode-Neutral and Require `OPTICS_STATIC=1` Everywhere

- **Why considered**: matches ADR-0015 and avoids setting `static` in the
  manifest.
- **Why rejected**: every release invocation, packaging script, and canonical
  verification command had to remember `OPTICS_STATIC=1`; omitting it silently
  produced a dynamically linked release binary on `default_library=both` hosts.

### 2. Maintain Different `Cargo.toml` Features on `main` vs `dev`

- **Why considered**: avoids `[env] OPTICS_DYNAMIC = "1"` in
  `.cargo/optics-local.toml`.
- **Why rejected**: permanent manifest divergence between `main` and `dev`
  causes recurring merge/squash conflicts during promotion and risks leaking
  non-static settings onto `main`.

---

## Links

- [ADR 0014: Optics Link Mode and Static Release Artifact](0014-optics-link-mode-and-static-release.md)
- [ADR 0015: Synchronize with Optics v0.0.83 and Shared-Preferred Linking](0015-synchronize-with-optics-v0.0.83-and-shared-preferred-linking.md)
- [Cross-Repository Development Workflow](../dev/cross-repository-development.md)
- [Build & Run Guide](../dev/build-and-run.md)
- [Distribution Packaging](../dev/packaging.md)
