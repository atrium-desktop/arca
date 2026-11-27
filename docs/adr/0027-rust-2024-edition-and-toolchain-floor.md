---
id: ADR-0027
title: "Rust 2024 Edition, Resolver 3, and the Rust 1.88 Toolchain Floor"
status: accepted
date: 2026-11-27
scope: [build/toolchain, code/conventions]
superseded_by: null
negative_knowledge: true
---

# 0027. Rust 2024 Edition, Resolver 3, and the Rust 1.88 Toolchain Floor

- Status: Accepted
- Date: 2026-11-27
- Deciders: Arca Architecture Working Group
- References: [Rust 2024 edition guide](https://doc.rust-lang.org/edition-guide/rust-2024/index.html)

---

## Context and Problem Statement

The workspace was on edition 2021 with `resolver = "2"` and
`rust-version = "1.85"`. The Optics bindings Arca builds from source are
already edition 2024. Edition 2024 changes the meaning of some existing code
(RPIT lifetime capture, temporary drop scopes, `unsafe` requirements on
`extern` blocks and environment writes). It also enables let chains, which
need Rust 1.88.

## Decision

### Workspace settings

- `edition = "2024"` for every member, through `[workspace.package]`.
- `resolver = "3"` in `[workspace]`. A virtual workspace does not infer its
  resolver from the edition, so this is set explicitly. Resolver 3 prefers
  dependency versions compatible with `rust-version`.
- `rust-version = "1.88"`, the first release with stable let chains. Clippy
  only suggests rewrites that need a newer toolchain when `rust-version`
  allows them, so the floor and the lints stay in step.
- Formatting follows the 2024 style edition, which `cargo fmt` takes from the
  package edition. It sorts imports by version, so most files changed only
  in their import order.

### Migration

`cargo fix --edition` ran on edition 2021 first, then the edition was
switched and `cargo clippy --fix` applied the newly available suggestions.
Every automated rewrite was reviewed:

- **`unsafe extern` blocks** for every libc declaration (`ipc.rs`,
  `watch.rs`, `trash.rs`, `telemetry/capture.rs`).
- **`use<>` precise capture** on `chooser::parse_class`, keeping its edition
  2021 capture set. The `'static` bound that `cargo fix` kept next to `use<>`
  added nothing and was removed.
- **Environment writes.** `std::env::set_var` and `remove_var` are `unsafe`
  in 2024. `cargo fix` wrapped each call and left a `FIXME`. Instead,
  `test-support::env` now makes every write through one private `write_env`
  helper. The helper is called only while `ENV_LOCK` is held. Its `SAFETY`
  comment states the real precondition: readers do not take the lock, so
  the write is sound only under nextest's process-per-test model
  ([ADR-0019](0019-explicit-test-resources.md)), not under threaded
  `cargo test`. No test outside `test-support`'s own self-tests uses
  `EnvGuard` or `XdgIsolation`.
  The `guard_restores_unset_state` test used to clear its variable without
  the lock. It now takes the lock like every other write.
- **Drop-order warnings (`tail_expr_drop_order`).** Five sites were flagged,
  all of the form `if let Ok(x) = f()` or `while let Ok(x) = rx.try_recv()`.
  The value moves into the binding, so the temporary that now drops earlier
  is an emptied `Result`. None was changed.
- **Let chains.** Clippy's `collapsible_if` collapsed 48 nested
  `if` / `if let` blocks into `if .. && let ..` chains. A chain could only
  change behaviour if one of its conditions held a lock guard or borrow that
  is later dropped. None of the 48 does.
- **`as_chunks`** (Rust 1.88) replaces `chunks_exact` in `arca-xdg`'s MD5. It
  yields `&[u8; 4]`, so the manual byte indexing is gone.

### Conventions from here on

1. **`safe fn` for FFI calls with no preconditions.** In an `unsafe extern`
   block, declare a function `safe` when its call cannot violate memory or
   I/O safety for any argument (`getuid`, `geteuid`, `inotify_init1`). Call
   sites then need no `unsafe` block. Functions that take pointers or
   existing file descriptors stay unsafe, and each call keeps its `SAFETY`
   comment.
2. **`#[expect(lint, reason = "..")]` instead of `#[allow]`.** An expectation
   warns when its lint stops firing, so stale suppressions show up. The
   migration removed one: the `unused_imports` allow on `icons::ids` never
   fired.
3. **Let chains instead of nested `if let`** when there is no `else` branch.
   Clippy enforces this.
4. **No new process-environment writes outside `test-support::env`.**
   Production code reads the environment and never writes it.

## Consequences

### Positive

- The workspace matches the Optics bindings and current Rust tooling.
- Fewer `unsafe` blocks and fewer suppressed lints, and each one left has a
  stated reason.

### Negative

- Building needs Rust 1.88 or newer (released 2025-06). CI uses stable, so
  only distribution packagers on very old toolchains are affected.
- The 2024 import order touches most files once, which makes `git blame`
  noisier for this commit.

### Negative knowledge

- `cargo fix --edition` does not deal with environment writes properly. It
  wraps each call in `unsafe` and leaves a `FIXME`, so its output is a to-do
  list, not a finished migration.
- Drop-order warnings come with no automatic fix. Each one needs reviewing,
  but the common `if let Ok(x) = f()` form is harmless.
