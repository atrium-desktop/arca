# Repository Governance

This document establishes the architectural charter, engineering invariants, and
review gates for Arca.

All contributors, maintainers, and automated agents must adhere to these policies
when authoring code or documentation.

---

## 1. Project Mission and Architectural Charter

Arca is a fast, keyboard-first file manager for Wayland and a first-party companion
application for the Tessera desktop ecosystem.

Its architecture is governed by four foundational principles:

1. **Strict Separation of Concerns**:
   - `crates/arca-core`: Owns 100% of domain business logic (filesystem traversal,
     bookmarks, sorting, filtering, batch operations, trash, and off-thread thumbnail
     decoding). It has zero GUI dependencies and must remain headless-testable.
   - `crates/arca-ui`: Owns presentation and event translation using the `lens`
     immediate-mode UI and `iris` Wayland runtime. It renders state provided by
     `arca-core` per frame without retaining separate business state.
   - `crates/arca`: Thin entrypoint binary relaying library search paths and CLI
     flags.

2. **Immediate-Mode Frame Budget**:
   - The UI runs on top of the Optics stack (`flux` Vulkan renderer, `lens` UI
     toolkit, `iris` application runtime).
   - Lens enforces a 1 MiB per-frame widget arena budget. Listing views (Grid,
     List, Miller columns) must remain strictly virtualized to guarantee bounded
     frame allocation regardless of directory size.

3. **Optics FFI Discipline**:
   - Prefer safe `lens` APIs whenever available.
   - When calling raw `lens_sys` symbols (widget focus, clipboard integration,
     custom icon drawing), isolate raw pointers inside centralized helper modules
     in `arca-ui` (`src/icons.rs`, `src/device.rs`, `src/menus.rs`).
   - Never sprinkle ad-hoc FFI calls across view rendering code.

4. **Desktop Interoperability**:
   - Conform to freedesktop specifications: XDG Base Directory, XDG User Dirs,
     FreeDesktop Trash specification, and `org.freedesktop.impl.portal.FileChooser`
     version 3.

---

## 2. Review Gates

Every pull request must pass four review gates before merge:

### Gate 1: Architecture & Decisions
- Does this change introduce a lasting architectural shift, protocol change, or
  new external dependency?
- If yes, record the rationale in an Architecture Decision Record under `docs/adr/`.
- Additional crates.io dependencies in `arca-*` require maintainer approval (the
  only approved external dependency is `image` for thumbnail decoding).

### Gate 2: Invariant & Correctness
- All business logic changes must include unit tests in `arca-core`.
- Rendering changes must pass headless frame tests in `arca-ui`.
- Virtualization guards must remain intact; no unbounded widget loops.
- `cargo test` must pass cleanly without warnings.

### Gate 3: Dual-Tier Validation
- Programmatic test suites must be updated per [Testing Guide](../dev/testing.md).
- End-to-end user journeys or scenario matrices must be updated in
  [Acceptance Guide](../dev/acceptance.md) if user-facing behavior changed.

### Gate 4: Documentation Governance
- Documentation must follow the 4-gate routing cascade defined in
  [Documentation Governance](documentation/core/routing.md).
- Changes must be categorized into Diátaxis surfaces (`tutorials/`, `how-to/`,
  `reference/`, `explanation/`) or developer internal surfaces (`dev/`).

---

## 3. Subsystem Governance Directory

- [Documentation Governance](documentation/core/index.md) — The portable
  documentation governance protocol (Protocol v3.1.0) and manifest verification.
- [Repository Contracts](documentation/contracts.md) — Declared profiles and path
  bindings for Arca.
