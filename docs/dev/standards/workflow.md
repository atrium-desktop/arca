# Engineering Workflow & Verification Gates

This document codifies the operational workflows, PR review gates, and verification criteria for changes to Arca.

---

## 1. Pull Request Review Gates

Before any pull request is merged into `main`, it must clear the following verification gates:

```text
┌─────────────────────────────────────────────────────────────┐
│ 1. Compilation Gate: `cargo check --workspace`              │
│    Zero warnings, zero unresolved symbols, all crates green │
├─────────────────────────────────────────────────────────────┤
│ 2. Automated Test Gate: `cargo test --workspace`            │
│    All unit, headless frame, and regression tests pass      │
├─────────────────────────────────────────────────────────────┤
│ 3. Architecture Gate: Subsystem Boundary Compliance         │
│    Engine headless purity, XDG zero-dependency purity,      │
│    1 MiB optics-ui arena virtualization maintained,         │
│    release artifact embeds Optics (INV-BUILD-01)            │
├─────────────────────────────────────────────────────────────┤
│ 4. Code-to-Doc Synchronization Gate                         │
│    API, CLI, shortcut, or protocol changes matched in docs  │
└─────────────────────────────────────────────────────────────┘
```

---

## 2. Code-to-Doc Synchronization (Atomic Commits)

Any pull request that introduces or modifies user-visible or integrator-facing surfaces must update the corresponding documentation within the **same atomic pull request**:

| Modified Surface | Required Documentation Synchronization |
| :--- | :--- |
| CLI commands, arguments, or flags (`crates/arca`) | `docs/reference/cli.md` |
| Configuration keys or defaults (`crates/arca-engine/src/config.rs`) | `docs/reference/configuration.md` |
| Portal prompter JSON contract (`crates/arca-engine/src/chooser.rs`) | `docs/reference/portal-protocol.md` |
| Keyboard shortcuts or navigation actions (`crates/arca-ui`) | `docs/reference/keyboard-shortcuts.md` |
| Major architectural restructuring or crate additions | New or amended ADR in `docs/adr/` and `docs/dev/subsystems.md` |

---

## 3. Pre-Release Quality Checklist

Prior to creating a release tag:

1. **Workspace Compilation**: Run `cargo check --workspace` and verify zero errors.
2. **Full Test Suite**: Run `cargo test --workspace` across all crates (ensuring `arca-xdg`, `arca-engine`, `arca-ui`, and `arca` pass).
3. **Packaging Integrity**: Verify that headless tests pass under `--locked` or offline flags (`cargo test --release --locked --workspace`).
4. **Release Link Integrity**: Run `OPTICS_STATIC=1 cargo build --release -p arca` and `scripts/check-optics-link.sh --expect static target/release/arca`; the artifact must embed the Optics modules (`[INV-BUILD-01]`).
5. **Changelog Integrity**: Ensure user-facing features, fixes, and migrations are documented in `CHANGELOG.md`.
6. **Clean Git State**: Verify no untracked experimental files or temporary debug assets remain in the tree.
