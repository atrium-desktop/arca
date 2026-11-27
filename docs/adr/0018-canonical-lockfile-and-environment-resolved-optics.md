---
id: ADR-0018
title: "Canonical Lockfile and Environment-Resolved Optics Checkout"
status: superseded
date: 2026-11-24
scope: [build/dependencies, build/lockfile, developer-workflow]
superseded_by: ADR-0024
negative_knowledge: true
---

# 0018. Canonical Lockfile and Environment-Resolved Optics Checkout

- Status: Superseded by [ADR-0024](0024-local-optics-source-patch-and-readonly-lockfile-guardrail.md)
- Date: 2026-11-24
- Deciders: Arca Architecture Working Group
- Consulted: Optics Build Maintainers, Release Engineering
- Amends: [ADR-0015](0015-synchronize-with-optics-v0.0.83-and-shared-preferred-linking.md), [ADR-0016](0016-default-static-optics-manifest-and-dev-dynamic-override.md)
- References: [Local Optics Development](../dev/optics-local-development.md), [Setup](../dev/setup.md)

---

## Context and Problem Statement

Arca's development workflow used a Cargo `[patch."https://github.com/ming2k/optics"]`
block in `.cargo/optics-local.toml` to redirect every Optics crate to a sibling
`../optics/crates/*` checkout. The consequences were structural:

1. **Two dependency graphs.** `main` resolved the tagged Git sources while
   `dev` resolved local paths, so `cargo tree`, feature unification, and
   `--locked` described different graphs.
2. **A lockfile that could not be committed from `dev`.** Cargo rewrites the
   path-patched entries, so the pre-commit hook had to unstage `Cargo.lock` and
   `main` had to regenerate it. A merge could carry a lockfile that no worktree
   had actually tested.
3. **A silent source-selection hazard.** A local patch can make a build use
   uncommitted Optics Rust code that the pinned tag does not contain; the
   mismatch surfaces only during promotion or in CI.
4. **Hook complexity.** The pre-commit hook had two modes, a release-shaped
   commit rejection, and an index-rewrite path. That is operational debt for a
   problem the build scripts already solve.

Optics `optics-build` already accepts `OPTICS_SOURCE_DIR` and `OPTICS_BUILD_DIR`
and discovers a Meson uninstalled tree without patching the Cargo graph. Arca
was not using that seam.

---

## Decision Drivers

- Driver 1: **One graph, one lockfile, every worktree.** `cargo check --locked`
  and `cargo metadata --locked` must mean the same thing in `main`, `dev`, and
  CI.
- Driver 2: **Committable canonical state.** A developer must be able to commit
  a lockfile change from any worktree without a hook rewriting the index.
- Driver 3: **Live native iteration.** C and shader edits in `../optics` must
  still take effect without a Rust relink.
- Driver 4: **No hidden source selection.** Rust code must come from the
  pinned tag; only the native build tree may come from a local checkout.
- Driver 5: **Simple enforcement.** The hook should validate the artifact, not
  infer a mode from a patch block.

---

## Architectural Decisions

### 1. Remove `[patch]` from the local configuration

`.cargo/optics-local.toml` and every generated `.cargo/config.toml` no longer
contain a `[patch]` table. The committed `Cargo.lock` always resolves the tagged
Git sources:

```toml
[env]
OPTICS_SOURCE_DIR = { value = "../optics", relative = true }
OPTICS_BUILD_DIR = { value = "../optics/build", relative = true }
OPTICS_DYNAMIC = "1"
```

The relative paths resolve against the workspace root, so a worktree at
`projects/arca-dev/` points at `projects/optics/`.

### 2. Native discovery is an environment input

The `optics-*-sys` build scripts use the sibling source/build tree to generate
bindings and locate the Meson uninstalled `.pc` files. The Rust crate sources
still come from the pinned Git tag. `OPTICS_DYNAMIC=1` keeps the shared-object
fast path; `OPTICS_DYNAMIC=0` verifies the static closure.

### 3. One canonical lockfile, committed everywhere

`dev` commits the same `Cargo.lock` as `main`. Promotion no longer restores or
regenerates a lockfile from a different worktree; it updates the tag and
regenerates the lock against the published tag in one atomic change.

### 4. The pre-commit hook validates, it does not rewrite

When a `Cargo.toml` or `Cargo.lock` is staged, the hook runs
`cargo metadata --locked`. It refuses a staged `.cargo/config.toml` (a
machine-local pointer) and never unstages or restores files. There is one mode.

### 5. CI uses the same environment seam

CI checks out the pinned Optics tag into a sibling path, builds its Meson tree
with `-Ddefault_library=both`, and exports `OPTICS_SOURCE_DIR` /
`OPTICS_BUILD_DIR`. Tests run through nextest; there is no CI-only patch or
lockfile variant.

---

## Consequences

### Positive

- `cargo tree -i optics-app` shows the pinned Git source in every worktree.
- `Cargo.lock` is a normal tracked file; no hook index rewriting, no
  local/main lock divergence, no unreviewed local Rust sources.
- Live Optics C/shader iteration is unchanged: only the build-tree pointer and
  link mode are local.
- CI and release verification can use `--locked` without a pre-patch step.

### Negative & Trade-offs

- The local workflow now depends on `optics-build`'s environment discovery;
  a sibling checkout that is not a Meson build tree fails with the upstream
  remediation message instead of being silently patched in.
- Developing new **Optics Rust binding APIs** still requires a temporary
  Cargo override. That is intentional: it must be explicit, reviewed, and not
  part of the committed workflow. Use a local `cargo --config` invocation or a
  throwaway lockfile and never commit it.
- `Cargo.lock` conflicts during rebases are now normal manifest conflicts
  rather than a local-mode artifact.

---

## Rejected Alternatives & Negative Knowledge

### 1. Keep the `[patch]` and unstage the lockfile

- **Why considered**: zero workflow change.
- **Why rejected**: it keeps two graphs and makes the canonical lockfile
  untestable from the worktree that generated the change. The hook's exclusion
  logic was the symptom.

### 2. Use a separate lockfile path for `dev`

- **Why considered**: isolates local resolution without a patch.
- **Why rejected**: Cargo has no supported alternate lockfile path; wrappers
  that manipulate `--manifest-path`/`CARGO_TARGET_DIR` make builds
  unreproducible and hide the graph from tooling.

### 3. Install Optics into a prefix and use only `pkg-config`

- **Why considered**: closest to the canonical contributor workflow.
- **Why rejected**: it removes fast iteration: every C/shader edit needs an
  install step and the uninstalled Meson tree is the supported development
  seam. It remains the canonical contributor path when no sibling checkout
  exists.

### 4. Patch only the `-sys` crates

- **Why considered**: avoids local Rust wrapper sources.
- **Why rejected**: `[patch]` operates on the Cargo graph; patching only some
  members still forks feature unification and the lockfile. The environment
  seam selects the native tree without touching the graph at all.

---

## Links

- [ADR 0015: Synchronize with Optics v0.0.83](0015-synchronize-with-optics-v0.0.83-and-shared-preferred-linking.md)
- [ADR 0016: Default-Static Optics Manifest and Worktree-Local Dynamic Override](0016-default-static-optics-manifest-and-dev-dynamic-override.md)
- [ADR 0017: Synchronize with Optics v0.0.86 and the Root Test Architecture](0017-synchronize-with-optics-v0.0.86-and-root-test-architecture.md)
- [Local Optics Development](../dev/optics-local-development.md)
- [Setup](../dev/setup.md)
