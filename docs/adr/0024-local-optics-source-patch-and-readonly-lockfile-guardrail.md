---
id: ADR-0024
title: "Local Optics Source Patch and Read-Only Lockfile Guardrail"
status: accepted
date: 2026-11-27
scope: [build/dependencies, build/lockfile, developer-workflow]
superseded_by: null
negative_knowledge: true
---

# 0024. Local Optics Source Patch and Read-Only Lockfile Guardrail

- Status: Accepted
- Date: 2026-11-27
- Deciders: Arca Architecture Working Group
- Supersedes: [ADR-0018](0018-canonical-lockfile-and-environment-resolved-optics.md)
- References: [Local Optics Development](../dev/optics-local-development.md), [Setup](../dev/setup.md)

---

## Context and Problem Statement

[ADR-0018](0018-canonical-lockfile-and-environment-resolved-optics.md) removed
Cargo `[patch."https://github.com/ming2k/optics"]` from `.cargo/optics-local.toml`
so that `dev` and `main` would share one canonical `Cargo.lock`. Under that
design, `arca-dev` compiled Rust bindings from the remote Git tag pinned in
`Cargo.toml` while resolving native C headers and shared libraries from the
sibling `../optics/build` tree via `OPTICS_SOURCE_DIR` and `OPTICS_BUILD_DIR`.

In practice, that split broke local development whenever `../optics` moved to a
new pre-1.0 version (for example `v0.0.86` -> `v0.0.87`):

1. **Upstream exact-version invariant.** `optics-build` intentionally probes
   native libraries with `pkg-config --exact-version` against the `-sys`
   crate's `CARGO_PKG_VERSION`, and pre-1.0 Optics SONAMEs carry the full
   version. Weakening that check in `optics` would expose every downstream
   consumer to silent pre-1.0 ABI/behavior skew.
2. **Half-local source skew.** Keeping Rust wrappers pinned to an older remote
   tag while linking a newer local `../optics/build` broke `cargo run` /
   `cargo build` as soon as `../optics` bumped its version and Cargo re-ran
   build scripts, blocking unrelated Arca work in `arca-dev`.
3. **Pre-1.0 co-evolution.** During `0.0.x`, Optics C changes and Rust binding
   changes frequently land together. Forcing manual `cargo --config` overrides
   for Rust edits defeated the purpose of a dedicated `arca-dev` worktree.

---

## Decision Outcome

Restore Cargo `[patch."https://github.com/ming2k/optics"]` in
`.cargo/optics-local.toml` alongside `[env]` (`OPTICS_SOURCE_DIR`,
`OPTICS_BUILD_DIR`, `OPTICS_DYNAMIC = "1"`), and replace the hook's old
index-rewriting behaviour with a **strictly read-only pre-commit guardrail**:

1. **Co-versioned local `dev` builds.** `.cargo/optics-local.toml` patches
   every Optics crate in Arca's dependency closure to `../optics/crates/*`.
   Rust wrappers, `-sys` crates, C headers, and `liboptics_*.so` therefore
   always match the sibling checkout's exact version and commit. When
   `../optics` bumps its version, a single `cargo update -p optics-...` in
   `arca-dev` advances the local lockfile without touching `Cargo.toml`.
2. **Accept local `Cargo.lock` drift in `dev` as an explicit trade-off.**
   Canonical `Cargo.lock` ownership belongs to the primary `main` worktree
   (`../arca/`) and CI, which never have `.cargo/config.toml`.
3. **Read-only `.githooks/pre-commit` guardrail.** Unlike the pre-ADR-0018
   hook (which silently ran `git restore --staged`), the hook never mutates the
   Git index:
   - Always refuses a staged `.cargo/config.toml`.
   - In local patch mode, refuses a staged `Cargo.lock` or a release-shaped
     version bump in `Cargo.toml`, printing the exact remediation command.
   - In canonical mode (no Optics `[patch]`), validates
     `cargo metadata --locked --format-version 1` whenever `Cargo.toml` or
     `Cargo.lock` is staged.
4. **No changes to upstream `optics`.** `optics-build` keeps its strict
   `--exact-version` probe and full-version pre-1.0 SONAME contract intact.

---

## Consequences

### Positive

- `arca-dev` compiles and dynamically links cleanly against `../optics` even
  when `../optics` is ahead of the tag pinned in `Cargo.toml`.
- C, shader, and Rust binding edits in `../optics` work seamlessly in one loop.
- The pre-commit guardrail stays simple, predictable, and never surprises
  developers by silently editing staged files.
- `main` and CI continue to enforce `cargo check --locked --workspace` and
  static release linking (`scripts/check-optics-link.sh --expect static`).

### Negative & Trade-offs

- `git status` in `arca-dev` normally shows `M Cargo.lock` while the local
  patch is active; developers must keep `Cargo.lock` unstaged in `dev` and
  run `git checkout -- Cargo.lock` before rebasing or resetting onto `main`.
- Adding or updating non-Optics dependencies still requires regenerating and
  committing the canonical `Cargo.lock` from the unpatched `main` worktree
  (`../arca/`).
- `.cargo/optics-local.toml`'s `[patch]` table must include every Optics crate
  in Arca's transitive closure (until Arca optionally adopts the unified
  `optics` facade crate).

---

## Rejected Alternatives & Negative Knowledge

### 1. Relax `optics-build`'s `--exact-version` check for `OPTICS_BUILD_DIR`

- **Why considered**: would let `arca-dev` link `v0.0.87` native libraries with
  `v0.0.86` Rust crates without restoring `[patch]`.
- **Why rejected**: Optics is pre-1.0 and explicitly treats mismatched Rust
  wrappers and C libraries as unsupported. Weakening the upstream library's
  invariant to accommodate a half-local consumer setup risks silent runtime
  misbehaviour across all Optics consumers.

### 2. Restore the old auto-unstaging pre-commit hook

- **Why considered**: avoids a rejected commit when running `git add -A` in
  `arca-dev`.
- **Why rejected**: silently mutating the Git index inside a commit hook hides
  what is being committed and was a key source of confusion documented in
  ADR-0018. Failing fast with an explicit message is safer and easier to reason
  about.
