---
id: ADR-0017
title: "Synchronize with Optics v0.0.86 and Adopt the Root Test Architecture"
status: accepted
date: 2026-11-24
scope: [build/dependencies, rendering/images, quality/testing]
superseded_by: null
negative_knowledge: true
---

# 0017. Synchronize with Optics v0.0.86 and Adopt the Root Test Architecture

- Status: Accepted
- Date: 2026-11-24
- Deciders: Arca Architecture Working Group
- Consulted: Optics Graphics/Bindings Maintainers
- Extends: [ADR-0013](0013-synchronize-with-optics-v0.0.82-semantic-module-naming.md), [ADR-0015](0015-synchronize-with-optics-v0.0.83-and-shared-preferred-linking.md), [ADR-0016](0016-default-static-optics-manifest-and-dev-dynamic-override.md)
- References: Optics `v0.0.84`, `v0.0.85`, `v0.0.86`

---

## Context and Problem Statement

Arca was pinned to Optics `v0.0.83`. Three maintenance releases landed since:

1. **`v0.0.84` — `-sys` type ownership and static CI.** The Rust FFI crates
   unified `optics_gfx_*` type re-exports (one owner, re-exported by text,
   scene, SVG, and UI), completed dependency prefixes and include-path
   discovery for `ui`/`app`, relayed rpaths consistently in
   `optics-material`, and added a static-link pass to Optics CI. None of these
   change Arca's call sites, but they remove the possibility of two `-sys`
   crates disagreeing about a shared FFI type during a static build.
2. **`v0.0.85` — image handles across DisplayList replay.** Immutable image
   snapshots now resolve to the original image's canonical RenderPlan handle
   until a copy-on-write split gives them a new pixel version; deferred-pass
   pending state is mirrored in the shared image block; and `vk_image`
   lifetime observation covers the shared owner instead of each wrapper
   release. This is directly relevant to Arca: thumbnails are uploaded as
   `optics_gfx_image`s, evicted/released by a bounded LRU store, and referenced
   from recorded DisplayLists. The old disagreement could miss the pass a
   `canvas_begin` flushed or observe a resource release at the wrong time.
3. **`v0.0.86` — tooling simplification plus layered test growth.** Optics
   retired several over-engineered tree guardrails, expanded its unit, GPU
   integration, and public-API E2E suites, and fixed the Rust link-mode
   environment switches so `"0"`/`"false"` disable a worktree `[env]` entry.

At the same time, Arca's verification had drifted into crate-local files: the
damage/quiescence suite lived inside `arca-ui/tests`, architecture firewalls
lived only in CI YAML, and there was no process-level suite for the prompter or
the single-instance socket. The user-visible freeze under repeated clicking
also had no regression guard.

---

## Decision Drivers

- Driver 1: **Correct image lifetimes under replay.** A pinned release that
  still contains the pre-`v0.0.85` plan-handle disagreement is a poor foundation
  for a thumbnail-heavy grid.
- Driver 2: **One tested artifact.** Canonical, release, and local builds must
  resolve the same binding versions; the local `OPTICS_SOURCE_DIR` /
  `OPTICS_BUILD_DIR` override must not silently test a newer tree than the tag.
- Driver 3: **Executable architecture.** A boundary rule that only exists in CI
  YAML is invisible to a contributor running `cargo test`.
- Driver 4: **Headless, bounded, deterministic coverage.** Tests must not need
  a compositor or GPU, and a regression must fail rather than hang.
- Driver 5: **No source churn without a call-site reason.** Adopt upstream
  changes where Arca names the changed item; otherwise record the audit.

---

## Architectural Decisions

### 1. Repin to `v0.0.86`

`Cargo.toml` tracks `optics-app`, `optics-window`, `optics-ui`, `optics-ui-sys`,
`optics-gfx-sys`, and `optics-motion` at `v0.0.86`. The crate set, semantic
module names, and the repository-root `crates/optics-<module>` layout are
unchanged; the local `.cargo/optics-local.toml` template only points the
build scripts at the sibling checkout. Canonical
and linked development builds converge on the same version, and the pinned
release includes the image-plan and `-sys` ownership fixes.

No Arca source change was required: the `v0.0.84` FFI re-exports are internal to
the binding crates, `v0.0.85` preserves the C/Rust image API Arca calls, and
`v0.0.86` changes tooling and its own test suites. Arca continues to use
`optics_ui`, `optics_window`, `optics_app`, `optics_motion`, and the narrow
`optics_gfx_sys` image surface.

### 2. Keep the default-static manifest and the dev override

[ADR-0016](0016-default-static-optics-manifest-and-dev-dynamic-override.md) is
unchanged: the manifest enables the additive `static` feature, a linked
`dev` worktree sets `OPTICS_DYNAMIC=1`, and `OPTICS_DYNAMIC=0` verifies the
production closure on demand. Optics `v0.0.86` accepts `"0"`/`"false"` as
disabled values, which is what makes the documented shell override work.

### 3. One root external-test package, two binaries

Adopt the layered arrangement proven in the sibling desktop repository:

```text
crates/test-support/   std-only fixtures (scratch, env, poll, workspace)
tests/                      workspace member `arca-tests`, publish = false
  common/                    re-exports support + headless UiApp harness
  integration/main.rs        one link: architecture + engine/XDG + UI journeys
  e2e/main.rs                one link: spawned binary (CLI, prompter, IPC)
```

- **Unit tests stay beside their modules.** They may use private state; that is
  the point of the innermost layer.
- **Workspace integration owns boundaries.** Architecture firewalls
  (`architecture.rs`), navigation/file operations across `arca-engine` and
  `arca-xdg`, the prompter wire contract, the IPC socket, complete headless
  journeys, damage/quiescence, virtualization, and repetitive-click stress.
- **E2E owns processes.** The binary is spawned and driven through argv, stdio,
  and sockets. Tests never invoke `cargo build`; `nextest` builds workspace
  binaries once in a setup script, and the helper fails with the exact
  remediation if it is missing.
- **`native` feature.** The presentation suites are gated so
  `--no-default-features` runs the pure architecture/engine/XDG/protocol lane on
  hosts without the Optics toolchain. `test-support` is std-only for the
  same reason.

### 4. Name the click-stall invariant

A directory open must not request an XDG activation token. optics-window's token
call is synchronous and bounded at roughly one second; requesting it to navigate
made every folder double-click pay the timeout on compositors that delay
`done`. Arca now resolves the target first and requests a token only for an
application launch. The "Open with" candidate list is cached per MIME type
instead of re-parsing the desktop database on every right-click. The stress
suite bounds hundreds of clicks and view changes, and
[Interaction Latency](../dev/interaction-latency.md) records the remaining
synchronous paths (navigation, first desktop scan, file launch) and the profiling
recipe.

---

## Consequences

### Positive

- Release and development builds share one pinned Optics version that includes
  the image-plan-handle and shared-`VkImage` lifetime fixes.
- `cargo test --workspace` now enforces the architecture red-lines, the prompter
  contract, the IPC handshake, virtualized rendering, and click latency without
  a compositor.
- A real deadlock fails under nextest's timeout instead of hanging CI.
- The native-free lane still gives fast feedback on a host that cannot build
  the C stack.

### Negative & Trade-offs

- The root suites link the presentation stack by default, so the default test
  build needs the Optics toolchain. `--no-default-features` is the escape hatch.
- The "Open with" cache does not observe applications installed while the
  window is open; reopening Arca refreshes it.
- Navigation and the first association lookup remain synchronous. They are
  documented residuals, not solved by this ADR; an off-thread read/discovery
  job is the next step if profiling justifies it.
- The end-to-end suite requires built workspace binaries. Plain `cargo test
  --workspace` builds them as part of the run; a direct `cargo test -p
  arca-tests` requires `cargo build --workspace --bins` first (nextest automates
  it).

---

## Rejected Alternatives & Negative Knowledge

### 1. Stay on `v0.0.83`

- **Why considered**: no manifest or documentation change.
- **Why rejected**: the pinned source would retain the pre-`v0.0.85` image-plan
  handle disagreement and per-wrapper `vk_image` lifetime observation, exactly
  the code paths Arca's thumbnail store exercises while a DisplayList is replayed.

### 2. Keep crate-local integration tests

- **Why considered**: no new workspace member; tests sit beside their crate.
- **Why rejected**: every crate links its own harness binary, architecture
  rules stay in CI YAML, and process-level tests have no natural home. One
  external package is the single place to look for boundary coverage.

### 3. Run `cargo build` from the E2E tests

- **Why considered**: makes the suite self-contained.
- **Why rejected**: the test runner holds Cargo's target-directory lock; a
  nested build blocks until the timeout and mimics the very freeze the suite is
  meant to detect. Setup scripts are the supported mechanism.

### 4. Drop activation tokens entirely

- **Why considered**: removes the last synchronous platform roundtrip from the
  open path.
- **Why rejected**: tokens are how a compositor gives focus to the launched
  application. The correct scope is "only when launching", which the helper now
  enforces; dropping them would regress focus behaviour for files.

### 5. Make navigation asynchronous in this change

- **Why considered**: removes the largest remaining UI-thread filesystem cost.
- **Why rejected**: it changes the engine's state-machine contract, selection
  semantics, and watcher interaction. That deserves its own ADR and a
  generation-counter design, not a rider on a dependency bump.

---

## Links

- [ADR 0013: Synchronize with Optics v0.0.82](0013-synchronize-with-optics-v0.0.82-semantic-module-naming.md)
- [ADR 0015: Synchronize with Optics v0.0.83](0015-synchronize-with-optics-v0.0.83-and-shared-preferred-linking.md)
- [ADR 0016: Default-Static Optics Manifest and Worktree-Local Dynamic Override](0016-default-static-optics-manifest-and-dev-dynamic-override.md)
- [Automated Testing](../dev/testing.md)
- [Interaction Latency](../dev/interaction-latency.md)
- [Local Optics Development](../dev/optics-local-development.md)
