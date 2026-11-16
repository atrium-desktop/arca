# Contributor Documentation

This directory contains internal engineering documentation for contributors working on Arca: build workflows, automated testing, manual acceptance runbooks, packaging, architecture blueprints, and engineering standards.

For the user-facing project overview, see the root [README](../../README.md). For agent directives and subsystem red-lines, see [AGENTS.md](../../AGENTS.md).

---

## Developer Guides & Runbooks

| Page | Topic |
| :--- | :--- |
| **[Setup](setup.md)** | Prerequisites, canonical vs local workflows, building native dependencies, running, and troubleshooting |
| **[Build & Run Guide](build-and-run.md)** | Compiling, sandbox execution, CLI picker mode, portal prompter, and headless tests |
| **[Optics Development Worktree](optics-dev-worktree.md)** | Developing against a live sibling `optics` checkout via Cargo `[patch]` and linked worktrees |
| **[Automated Testing](testing.md)** | Four-tier test pyramid, running unit and headless lens frame tests, and invariant validation |
| **[Acceptance Scenarios](acceptance.md)** | Outside-in deliverable acceptance criteria, end-to-end user journeys, and scenario matrices |
| **[Release & Versioning](release-and-versioning.md)** | Lockstep workspace SemVer model, pre-release checklists, and Git tagging playbooks |
| **[Distribution Packaging](packaging.md)** | Packaging guidelines, dependencies, reproducible builds, and system desktop manifests |
| **[Optics Migration History](optics-migration.md)** | History and mechanics of porting to the Optics monorepo (`aperture` / `flux` / `lens` / `iris`) |

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
