# Agent Directives

## 1. Operational Guardrails (Hard Constraints)
- **Prohibited Git Commands**: NEVER execute `git stash`, `git branch`, or `git reset`.
- **Command Termination**: All commands MUST be finite and self-terminating. Never launch unbounded interactive sessions or blocking loops without a strict timeout.
- **Headless Execution**: Verification commands must NOT assume an active Wayland compositor is connected. Drive UI tests headlessly via `lens::Ui::headless()` and `cargo test --workspace`.

## 2. Core Mental Model & Subsystem Boundaries
- **Concentric Architecture**: Arca is organized into four concentric layers with a single public executable binary (`arca`).
- **XDG Specification Purity**: `crates/arca-xdg` is strictly zero-dependency (`std` only). It must never depend on any workspace member or external crates.io package.
- **Engine Headless Purity**: `crates/arca-engine` owns all business logic, session state machines, directory models, sorting/filtering, and file operations. It must never link `lens`, `iris`, `flux`, or any graphical/Wayland windowing runtime.
- **Lens Frame Arena Budget & Virtualization**: `crates/arca-ui` enforces the 1 MiB immediate-mode widget arena budget. All listing views (**Grid**, **List**, **Miller columns**) must remain strictly virtualized. Unbounded widget allocation loops over directory collections are strictly prohibited.
- **Non-Destructive File Operations**: File deletions must default to compliant FreeDesktop trash with companion `.trashinfo` metadata. Inline renaming and folder creation must never silently clobber existing destinations.

## 3. Engineering Discipline
- **Mechanisms Over Exhortations**: Rely on Rust compiler typestates, private visibility, Cargo dependency firewalls, and automated tests to guarantee invariants. Never rely on natural-language comments alone.
- **Anti-Inflation**: Do not mint pseudo-invariant tags (e.g. `INV-UI-PADDING`, `INV-COLOR-*`). Model state via typed enums and visual layout metrics via design constants.
- **Atomic Verification**: Always verify code changes with `cargo check --workspace` and relevant crate test suites (`cargo test --workspace`) before declaring tasks complete.
- **Clean Epistemic Lineage**: References must resolve to canonical in-repo ADRs (`docs/adr/0001`+). Never reference non-existent external legacy numbers.

## 4. On-Demand Standards Routing
Do not guess repository rules or rely on stale assumptions. Consult authoritative specifications in `docs/dev/standards/` as needed:
- Workspace Topology & System Red-Lines: `docs/dev/standards/architecture.md`
- Invariant & Constraint Policy: `docs/dev/standards/invariants.md`
- ADR Authoring & Lifecycle: `docs/dev/standards/adr.md`
- RFC Standards: `docs/dev/standards/rfc.md`
- Documentation Taxonomy & Firewalls: `docs/dev/standards/taxonomy.md`
- Markdown & Hyperlink Style: `docs/dev/standards/style.md`
- PR Gates & Code-Doc Sync: `docs/dev/standards/workflow.md`
- Blameless Postmortems: `docs/dev/standards/postmortems.md`
- Testing Strategy & Execution: `docs/dev/testing.md`
- Subsystem Architecture Blueprint: `docs/dev/subsystems.md`
