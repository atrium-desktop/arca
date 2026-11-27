# Architecture Decision Records

This directory contains the recorded Architecture Decision Records (ADRs) for
Arca.

ADRs record significant architectural and design choices, their context, and
their consequences. Once accepted, records are immutable; subsequent changes are
documented as new ADRs that supersede older ones.

For the record template and guidelines, see [ADR Template](template.md).

## Registered Records

| Number | Title | Status | Date |
|--------|-------|--------|------|
| [0001](0001-optics-stack-migration.md) | Optics Stack Migration and Workspace Modularization | Accepted | 2026-08-20 |
| [0002](0002-xdg-desktop-portal-prompter.md) | XDG Desktop Portal FileChooser Prompter Protocol | Accepted (Amended by ADR-0023) | 2026-09-02 |
| [0003](0003-borderless-chrome-and-dense-hierarchy.md) | Borderless Surfaces, Subtle Controls, and Visual Density Hierarchy | Accepted (Amended by ADR-0005) | 2026-09-10 |
| [0004](0004-optics-v0.0.60-motion-and-image-pipeline-alignment.md) | Optics v0.0.60 Motion and Premultiplied Image Upload Alignment | Accepted | 2026-10-17 |
| [0005](0005-fluid-adaptive-grid-and-typographic-metrics.md) | Fluid Adaptive Grid Architecture and Derived Typographic Metrics | Accepted | 2026-10-18 |
| [0006](0006-optics-v0.0.64-segmented-controls-and-orthogonal-icon-metrics.md) | Optics v0.0.64 Segmented Controls, Orthogonal Icon Sizing, and Presentation Pacing Alignment | Accepted | 2026-10-31 |
| [0007](0007-optics-v0.0.66-compound-patterns-and-textedit-interaction-alignment.md) | Optics v0.0.66 Compound Patterns, TextEdit Interaction, and Desktop Cursor Alignment | Accepted | 2026-10-31 |
| [0008](0008-synchronize-with-optics-v0.0.74-and-damage-quiescence.md) | Synchronize with Optics v0.0.74, Regional Damage Tracking, and Quiescent Presentation | Accepted | 2026-11-06 |
| [0009](0009-synchronize-with-optics-v0.0.76-and-native-frame-damage.md) | Synchronize with Optics v0.0.76, Native Frame Damage, and Caret Phase Semantics | Accepted | 2026-11-10 |
| [0010](0010-synchronize-with-optics-v0.0.78-and-record-only-host-canvas.md) | Synchronize with Optics v0.0.78 and Retire the Live-Canvas Host Path | Accepted | 2026-11-12 |
| [0011](0011-synchronize-with-optics-v0.0.79-and-unified-bindings.md) | Synchronize with Optics v0.0.79, Unified Bindings, and Pure-Rust Motion | Accepted (Amended by ADR-0025) | 2026-11-12 |
| [0012](0012-synchronize-with-optics-v0.0.80-and-aperture-shell.md) | Synchronize with Optics v0.0.80, the Aperture Application Shell, and Descriptor-Only Operations | Accepted | 2026-11-16 |
| [0013](0013-synchronize-with-optics-v0.0.82-semantic-module-naming.md) | Synchronize with Optics v0.0.82, Semantic Module Naming, and the Zero-Stutter Namespace | Accepted | 2026-11-20 |
| [0014](0014-optics-link-mode-and-static-release.md) | Optics Link Mode: Library-Level Flexibility, Static Release Artifact | Accepted | 2026-11-20 |
| [0015](0015-synchronize-with-optics-v0.0.83-and-shared-preferred-linking.md) | Synchronize with Optics v0.0.83, Shared-Preferred Linking, and the Additive Static Feature | Accepted (Amended by ADR-0016, ADR-0018) | 2026-11-23 |
| [0016](0016-default-static-optics-manifest-and-dev-dynamic-override.md) | Default-Static Optics Manifest and Worktree-Local Dynamic Override | Accepted (Amended by ADR-0018) | 2026-11-24 |
| [0017](0017-synchronize-with-optics-v0.0.86-and-root-test-architecture.md) | Synchronize with Optics v0.0.86 and Adopt the Root Test Architecture | Accepted | 2026-11-24 |
| [0018](0018-canonical-lockfile-and-environment-resolved-optics.md) | Canonical Lockfile and Environment-Resolved Optics Checkout | Superseded by ADR-0024 | 2026-11-24 |
| [0019](0019-explicit-test-resources.md) | Explicit Test Resources for Thumbnail Cache and Trash | Accepted | 2026-11-24 |
| [0020](0020-asynchronous-directory-navigation.md) | Asynchronous Directory Navigation at the UI Boundary | Accepted | 2026-11-24 |
| [0021](0021-local-first-observability.md) | Local-First Observability: Logs, APM Spans, Watchdog Alerts, and Diagnostics | Accepted | 2026-11-25 |
| [0022](0022-grid-default-and-centred-miller-columns.md) | Grid as the Default View and Centred Miller Columns | Accepted | 2026-11-25 |
| [0023](0023-chooser-contract-v1-hardened-ipc-and-strict-cli.md) | Chooser Contract v1, Hardened Single-Instance IPC v2, and Strict CLI Semantics | Accepted | 2026-11-25 |
| [0024](0024-local-optics-source-patch-and-readonly-lockfile-guardrail.md) | Local Optics Source Patch and Read-Only Lockfile Guardrail | Accepted | 2026-11-27 |
| [0025](0025-synchronize-with-optics-v0.0.88-and-app-owned-icon-decoding.md) | Synchronize with Optics v0.0.88: App-Owned SVG Decoding and Per-Context Icon Registration | Accepted | 2026-11-27 |
| [0026](0026-synchronize-with-optics-v0.0.89-and-single-representation-options.md) | Synchronize with Optics v0.0.89: Per-Corner Radii and Single-Representation Frame Options | Accepted | 2026-11-27 |
| [0027](0027-rust-2024-edition-and-toolchain-floor.md) | Rust 2024 Edition, Resolver 3, and the Rust 1.88 Toolchain Floor | Accepted | 2026-11-27 |
