# Crate Versioning & Release Operations Guide

This document defines the operational procedures, versioning invariants, Git tagging conventions, and release playbooks for all crates within the **Arca** repository.

---

## 1. Principles & The Lockstep Versioning Model

Arca maintains a unified, tightly integrated desktop application suite. Unlike autonomous multi-standard clusters with divergent release cadences, Arca operates under a **Unified Lockstep SemVer Model**:

1. **Workspace Version Alignment**: All internal crates (`arca-xdg`, `arca-engine`, `arca-ui`, `arca`) share the exact same version declared at the root manifest under `[workspace.package.version]`.
2. **Deterministic Release Bundles**: A release tag (e.g. `v0.2.10`) uniquely identifies an atomic snapshot across all crates, ensuring that package managers and distribution builders never experience version mismatch between engine, UI, and binary layers.
3. **Optics Stack Synchronization**: The repository declares upstream Optics bindings with explicit version tags matching the desktop runtime environment.

---

## 2. Workspace Crate Versioning Matrix

All four crates in the repository inherit their version from the root `Cargo.toml`:

| Crate | Layer | Role | Version Source |
| :--- | :--- | :--- | :--- |
| **`arca`** | Entrypoint | Primary Wayland application executable & portal prompter | `version.workspace = true` |
| **`arca-ui`** | Presentation | Optics app shell, `optics-ui` widgets, viewport virtualization, views | `version.workspace = true` |
| **`arca-engine`** | Domain Engine | Directory traversal, session state, safe operations, config | `version.workspace = true` |
| **`arca-xdg`** | Specification | Pure-std Freedesktop.org specifications and protocols | `version.workspace = true` |

Internal workspace dependencies are declared with both `path` and `version`:

```toml
[workspace.dependencies]
arca-xdg = { path = "crates/arca-xdg", version = "0.2.10" }
arca-engine = { path = "crates/arca-engine", version = "0.2.10" }
arca-ui = { path = "crates/arca-ui", version = "0.2.10" }
```

---

## 3. Pre-Release Verification Runbook

Before bumping the workspace version or tagging a release:

### 3.1 Workspace Compilation Gate
Run verification across the entire workspace:

```bash
cargo check --workspace
```
Expected result: zero warnings, zero unresolved symbols.

### 3.2 Automated Test Gate
Run full unit and headless frame test suites:

```bash
cargo test --workspace
```
Expected result: all unit tests in `arca-xdg`, `arca-engine`, and headless frame tests in `arca-ui` pass cleanly.

### 3.3 Offline / Locked Dependency Check
Ensure reproducible distribution builds:

```bash
cargo test --release --locked --workspace
```

### 3.4 Invariant & Arena Budget Verification
Verify that large directory listings stay strictly within the 1 MiB frame arena limit:

```bash
cargo test large_directory_does_not_overflow_frame_arena
```

---

## 4. Release Playbook (Step-by-Step)

### Step 1: Bump Workspace Version
Update the workspace version in `Cargo.toml`:

```toml
[workspace.package]
version = "0.2.11" # Update to target version
```

And update internal workspace dependencies in root `Cargo.toml`:

```toml
[workspace.dependencies]
arca-xdg = { path = "crates/arca-xdg", version = "0.2.11" }
arca-engine = { path = "crates/arca-engine", version = "0.2.11" }
arca-ui = { path = "crates/arca-ui", version = "0.2.11" }
```

### Step 2: Update Changelog
Record all user-facing changes, bug fixes, and architectural adjustments in `CHANGELOG.md` under the new version header.

### Step 3: Verify and Commit
```bash
cargo check --workspace
git add Cargo.toml Cargo.lock CHANGELOG.md
git commit -m "chore(release): prepare v0.2.11"
```

### Step 4: Create Git Tag
```bash
git tag -a v0.2.11 -m "Release v0.2.11"
```

### Step 5: Verify Package Artifacts
Ensure distribution packages build cleanly against the tagged release per [`docs/dev/packaging.md`](packaging.md).
