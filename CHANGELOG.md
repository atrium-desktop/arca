# Changelog

All notable changes to Arca will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

---

## [0.2.11] - 2026-10-18

### Added
- Fluid adaptive grid architecture and dynamic typographic cell metrics (ADR-0005).
- Multi-script line height measurement (Latin + CJK) ensuring mathematical clearance across all languages.
- Fluid column width allocation via `virtual_grid_calc` (104px–144px range) to eliminate dead margins and increase readable filename space.
- Fixed 50px art container ensuring stable horizontal baseline alignment across varying aspect ratios.
- Comprehensive automated regression and edge case test suites for grid virtualization.

### Fixed
- Fixed critical file name and picture preview overlay defect in grid view where 2-line labels collided with subsequent row thumbnails.
- Fixed hit-box desynchronization where clicking the second line of text dispatched events to the subsequent row.

---

## [0.2.10] - 2026-10-17

### Added
- Comprehensive engineering standards suite in `docs/dev/standards/` (`architecture.md`, `invariants.md`, `adr.md`, `rfc.md`, `taxonomy.md`, `style.md`, `workflow.md`, `postmortems.md`).
- Living subsystem architecture blueprint in `docs/dev/subsystems.md`.
- Manual testing runbook with cold-start sandboxing in `docs/dev/manual-testing.md`.
- Lockstep crate versioning guide in `docs/dev/release-and-versioning.md`.
- Chinese documentation (`README-zh.md`).
- ADR-0004: Alignment with Optics v0.0.60 motion dynamics (`transit`) and hardware premultiplied image upload pipeline.

### Changed
- Replaced nested governance directories with a flat, cohesive standards suite under `docs/dev/standards/`.
- Modernized `AGENTS.md` with explicit operational guardrails, mental model & boundaries, engineering discipline, and on-demand standards routing.
- Adopted Optics `transit` for spring dynamics and cubic easing in Quick Look animations.
- Refactored thumbnail GPU texture uploads to delegate premultiplication to `flux_image_update_region_premultiply`.

---

## [0.2.9] - 2026-09-28

### Added
- Tab drag-and-drop reordering with smooth visual positioning feedback.
- Clean application lifecycle shutdown handling integrated with `iris`.
- Synchronized workspace dependencies with Optics `v0.0.46`.

---

## [0.2.8] - 2026-09-18

### Added
- Optics extended function key navigation bindings.
- Wayland window activation token support (`iris::window_create_activation_token`) for focus stealing prevention.
- Refined focus management across location bar and inline rename editors.

---

## [0.2.7] - 2026-09-10

### Added
- Borderless floating chrome and subtle titlebars adhering to the Tessera design language (`ADR-0003`).
- Recessed input surfaces for location bar and search filter.
- Vibrant, crisp SVG icon set registered with Lens at runtime.

---

## [0.2.6] - 2026-09-05

### Added
- Live filesystem watching and automatic directory listing refresh on external changes.
- Default MIME application associations via `mimeapps.list` and `.desktop` parsing.
- Context menu stability improvements and hover interaction polish.

---

## [0.2.5] - 2026-08-30

### Added
- Location bar tab-completion for filesystem paths.
- Drag-and-drop file operations support.
- Improved contrast and typography for floating popovers and dialogs.

---

## [0.2.4] - 2026-08-25

### Added
- XDG Desktop Portal FileChooser label mnemonics parsing (`ADR-0002`).
- Fixed virtualized scroll boundary calculation in large directories.

---

## [0.2.3] - 2026-08-22

### Added
- Portal prompter JSON envelope updates matching Tessera Prompter Contract v6.
- Overwrite protection modal in file save dialogs.

---

## [0.2.2] - 2026-08-21

### Changed
- Major workspace restructuring: split legacy `arca-core` into pure-std `arca-xdg` and headless `arca-engine`.
- Implemented pure-std Freedesktop.org specifications: Base Directories, User Directories, RFC 8089 File URIs, Shared MIME-info, Desktop Entries, and FreeDesktop Trash v1.0.

---

## [0.2.1] - 2026-08-20

### Changed
- Bumped Optics dependencies to `v0.0.33`.
- Improved build script rpath relaying for native C library discovery without `LD_LIBRARY_PATH`.

---

## [0.2.0] - 2026-08-20

### Added
- Initial port from legacy C platform shim and `flux-ui` to the Optics monorepo (`flux`, `lens`, `iris`) (`ADR-0001`).
- Concentric Cargo workspace layout.
- Immediate-mode UI layout with 1 MiB frame arena memory budgeting and viewport row virtualization.
