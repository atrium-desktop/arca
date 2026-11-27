# Contributing to Arca

Thank you for your interest in contributing to Arca!

---

## 1. Contributor Guides and Workflow

Before starting work, please review the authoritative contributor and architecture documentation:

1. **Engineering Standards & Architecture**: Read [Arca Engineering Standards](docs/dev/standards/README.md), [System Architecture Governance](docs/dev/standards/architecture.md), and the living [Subsystem Blueprint](docs/dev/subsystems.md) for workspace topology, subsystem red-lines, and invariant policies.
2. **Development Environment & Setup**: See [Setup Guide](docs/dev/setup.md) and [Local Optics Development](docs/dev/optics-local-development.md) for setting up canonical builds or developing against a sibling Optics checkout.
3. **Automated Testing**: Review [Testing Guide](docs/dev/testing.md) for the four-tier architecture: crate-local unit tests, headless UI frames, the root `arca-tests` integration suite (architecture firewalls, cross-crate flows, stress/virtualization), and the spawned-binary end-to-end suite (`cargo test --workspace`; `cargo nextest run -p arca-tests` for per-test processes and the binary setup script).
4. **Build, Run & Acceptance**: Review [Build & Run Guide](docs/dev/build-and-run.md) and [Acceptance Guide](docs/dev/acceptance.md) for sandbox-isolated exploratory testing and outside-in user journey verification.
5. **Distribution Packaging**: See [Distribution Packaging](docs/dev/packaging.md) for packaging requirements, dependencies, and system manifests.

---

## 2. Pull Request Verification Gates

Every pull request must clear the four verification gates codified in [Engineering Workflow](docs/dev/standards/workflow.md):

1. **Compilation Gate**: `cargo check --workspace` must pass with zero errors or warnings.
2. **Automated Test Gate**: `cargo nextest run --locked --workspace` (canonical, process-per-test) must pass across all crates (`arca-xdg`, `arca-engine`, `arca-ui`, `arca`) and the test-only workspace members (`test-support`, `arca-tests`). `cargo test --locked --workspace` remains a supported local loop, and the native-free lane (`cargo nextest run -p arca-tests --no-default-features`) must also pass.
3. **Architecture Gate**: Strict compliance with subsystem boundaries — `arca-xdg` stays zero-dep pure-std, `arca-engine` stays strictly headless, `arca-ui` maintains the 1 MiB optics-ui frame arena budget with full virtualization, and the release `arca` artifact embeds Optics by default (`cargo build --release -p arca` plus `scripts/check-optics-link.sh --expect static`, `[INV-BUILD-01]`).
4. **Code-to-Doc Synchronization Gate**: Public CLI changes (`docs/reference/cli.md`), configuration keys (`docs/reference/configuration.md`), shortcuts (`docs/reference/keyboard-shortcuts.md`), or portal protocol updates (`docs/reference/portal-protocol.md`) must be committed in the same atomic change.

---

## 3. Documentation Standards

Public documentation follows the Diátaxis four-quadrant structure (`tutorials/`, `how-to/`, `reference/`, `explanation/`). Public documents must never link into internal developer guides (`docs/dev/**`). See [Documentation Taxonomy](docs/dev/standards/taxonomy.md) and [Documentation Style](docs/dev/standards/style.md) before authoring documentation.
