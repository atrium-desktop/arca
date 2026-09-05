# Contributor Documentation

This directory contains internal engineering documentation for contributors working
on Arca: build workflows, automated testing, delivery acceptance procedures, packaging,
and architecture notes.

For the project overview, see the root [README](../../README.md). For crate layout,
conventions, and AI assistant rules, see [AGENTS.md](../../AGENTS.md). For governance
charters, see [Repository Governance](../governance/index.md).

---

## Developer Guides

| Page | Topic |
|------|-------|
| [Setup](setup.md) | Prerequisites, canonical vs local workflows, building native dependencies, running, testing, and troubleshooting |
| [Cross-Repository Development](cross-repository-development.md) | Developing against a live sibling `optics` checkout via Cargo `[patch]` and linked worktrees |
| [Testing](testing.md) | Running, interpreting, and writing automated unit, integration, and headless UI frame tests |
| [Acceptance](acceptance.md) | Outside-in deliverable acceptance criteria, end-to-end user journeys, and scenario matrices |
| [Packaging](packaging.md) | Distribution packaging instructions, dependencies, reproducible builds, and system manifests |
| [Optics Migration](optics-migration.md) | History and mechanics of porting to the Optics monorepo (`flux` / `lens` / `iris`) |

---

## Governance and Architecture

- [Repository Governance](../governance/index.md) — Architectural invariants, review
  gates, and PR checklist.
- [Architecture Decision Records](../adr/index.md) — Durable technical decisions
  (ADRs).
- [Documentation Governance Standard](../governance/documentation/core/index.md) —
  Mirrored documentation governance standard (Protocol v3.1.0).
