---
id: ADR-0019
title: "Explicit Test Resources for Thumbnail Cache and Trash"
status: accepted
date: 2026-11-24
scope: [quality/testing, engine/state, presentation/ui]
superseded_by: null
negative_knowledge: true
---

# 0019. Explicit Test Resources for Thumbnail Cache and Trash

- Status: Accepted
- Date: 2026-11-24
- Deciders: Arca Architecture Working Group
- Consulted: Optics UI Maintainers, Release Engineering
- References: [Automated Testing](../dev/testing.md), [Interaction Latency](../dev/interaction-latency.md)

---

## Context and Problem Statement

Two test paths reached process-global, user-owned resources:

1. `UiApp::new` built a `ThumbStore::new`, whose `ThumbService::new` starts a
   janitor over `$XDG_CACHE_HOME/thumbnails/normal` (or `~/.cache`). Every UI
   test therefore attempted to prune the developer's real thumbnail cache, and
   a test could delete a file the desktop environment was still using.
2. `file_operations::trash_roundtrip_through_engine_and_undo` set `HOME` and
   every `XDG_*` variable to a scratch tree with a process-global lock so the
   engine's `trash_selected` would write there. Under plain `cargo test` the
   suite is multi-threaded in one process, so sibling tests could observe the
   mutated environment; under nextest each test is isolated, but the test
   still depended on a global.

Both are test-only symptoms of a product-shape issue: destructive or
side-effecting resources were hard-wired instead of injected.

---

## Decision Drivers

- Driver 1: **Tests must never mutate user state.** A `cargo test` run may not
  prune the real thumbnail cache or move files into the real Trash.
- Driver 2: **No process-global test protocol.** Test correctness should not
  depend on running one test per process or on a global environment lock.
- Driver 3: **Production defaults stay zero-config.** A normal `arca` run
  still uses the FreeDesktop cache and Trash locations.
- Driver 4: **Public, embeddable seams.** The same constructors are useful to
  an embedding host or a future integration test, not `#[cfg(test)]` hacks.

---

## Architectural Decisions

### 1. `UiApp` owns an injectable thumbnail cache directory

- `ThumbStore::with_cache_dir(path)` builds the service against an explicit
  directory; `ThumbStore::new()` keeps the FreeDesktop default.
- `UiApp::with_thumb_cache(state, cache_dir)` selects the explicit store;
  `UiApp::new` / `with_ipc` keep the default.
- The crate-local `test_support::ScratchDir` and the root `Harness` create a
  sibling cache directory and pass it to every test app. Damage, virtualization,
  status-bar, and thumbnail-store tests do the same.

### 2. `AppState` owns an injectable Trash root

- `AppState::with_config_and_trash_root(config_file, cfg, start_dir, root)`
  stores an optional root; `with_config` delegates with `None`.
- `trash_selected`, `trash_items`, `empty_trash`, restore, and purge resolve
  through that root (`list_trash_in_root`, `trash_paths_record(..., Some(root))`).
- The default remains the FreeDesktop home/per-mount Trash.
- The integration trash test uses a scratch root and inspects
  `list_trash_in_root`; it no longer mutates `HOME` or `XDG_*`.

### 3. Nextest is the canonical multi-process runner

`cargo test --workspace` remains supported for a fast local loop, but CI and
the documented full-suite command use nextest, which runs each test in its own
process. `XdgIsolation` stays in `test-support` for tests that deliberately
exercise environment-derived paths; it is no longer required by the product
Trash test.

---

## Consequences

### Positive

- A test run cannot prune the user's thumbnail cache or write to the user's
  Trash.
- The Trash test is deterministic under both `cargo test` and nextest; no
  cross-test environment race.
- Embedders can point destructive resources at a sandbox without patching
  globals.

### Negative & Trade-offs

- Two additional public constructors and one extra `AppState` field.
- Tests that forget the explicit constructor fall back to the real cache/Trash.
  The harness centralizes the correct choice, and code review should reject a
  new direct `UiApp::new` in tests.
- The janitor still runs for production, by design.

---

## Rejected Alternatives & Negative Knowledge

### 1. Keep `XdgIsolation` for the Trash test

- **Why considered**: no engine change.
- **Why rejected**: it leaves a process-global dependency and a latent
  `cargo test` race; isolation is only correct under one runner.

### 2. Add a `#[cfg(test)]` cache directory

- **Why considered**: smaller public surface.
- **Why rejected**: test-only branches change the code under test and cannot be
  used by embedders. The directory is a legitimate host input.

### 3. Disable the thumbnail janitor in tests

- **Why considered**: one flag.
- **Why rejected**: the tests must exercise the same service construction path
  as production; pointing it at a scratch directory preserves that while
  removing the side effect.

---

## Links

- [ADR 0017: Root Test Architecture](0017-synchronize-with-optics-v0.0.86-and-root-test-architecture.md)
- [Automated Testing](../dev/testing.md)
- [Interaction Latency](../dev/interaction-latency.md)
