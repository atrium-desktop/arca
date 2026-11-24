# Contributor Documentation

This directory contains internal engineering documentation for contributors working on Arca: build workflows, automated testing, manual acceptance runbooks, packaging, architecture blueprints, and engineering standards.

For the user-facing project overview, see the root [README](../../README.md).

---

## Developer Guides & Runbooks

| Page | Topic |
| :--- | :--- |
| **[Setup](setup.md)** | Prerequisites, canonical vs local workflows, building native dependencies, running, and troubleshooting |
| **[Build & Run Guide](build-and-run.md)** | Compiling, sandbox execution, CLI picker mode, portal prompter, and headless tests |
| **[Cross-Repository Development](cross-repository-development.md)** | Developing against a live sibling `optics` checkout via `OPTICS_SOURCE_DIR` / `OPTICS_BUILD_DIR`, linked worktrees, and dynamic-link overrides without patching the Cargo graph |
| **[Automated Testing](testing.md)** | Four-tier test architecture, root integration/E2E suites, native-free lane, and invariant validation |
| **[Interaction Latency](interaction-latency.md)** | Click-stall analysis, synchronous token/desktop-scan causes, and the stress guards that bound them |
| **[Acceptance Scenarios](acceptance.md)** | Outside-in deliverable acceptance criteria, end-to-end user journeys, and scenario matrices |
| **[Release & Versioning](release-and-versioning.md)** | Lockstep workspace SemVer model, pre-release checklists, and Git tagging playbooks |
| **[Distribution Packaging](packaging.md)** | Packaging guidelines, dependencies, reproducible builds, and system desktop manifests |
| **[Optics Migration History](optics-migration.md)** | History and mechanics of porting to the Optics monorepo (`optics-app` / `optics-gfx` / `optics-ui` / `optics-window`) |

---

## Architecture and Engineering Standards

- **[Subsystem Architecture Blueprint](subsystems.md)** — Living blueprint of workspace topology, crate boundaries, rendering subsystem, and invariant matrix.
- **[Engineering Standards](standards/README.md)** — Authoritative standards suite:
  - [Workspace Topology & Architecture](standards/architecture.md)
  - [Invariant Governance](standards/invariants.md)
  - [ADR Standards](standards/adr.md)
  - [RFC Standards](standards/rfc.md)
  - [Documentation Taxonomy](standards/taxonomy.md)
  - [Documentation Style & Hyperlinks](standards/style.md)
  - [Engineering Workflow & PR Gates](standards/workflow.md)
  - [Incident Postmortems](standards/postmortems.md)
- **[Architecture Decision Records (ADRs)](../adr/index.md)** — Durable, in-place architectural decisions and negative knowledge registry.
