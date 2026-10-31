---
id: ADR-0007
title: "0007. Optics v0.0.66 Compound Patterns, TextEdit Interaction, and Desktop Cursor Alignment"
status: accepted
date: 2026-10-31
scope: [ui/arca-ui]
superseded_by: null
negative_knowledge: true
---

# 0007. Optics v0.0.66 Compound Patterns, TextEdit Interaction, and Desktop Cursor Alignment

- Status: Accepted
- Date: 2026-10-31
- Deciders: Ming Li, Arca UI Architecture Working Group
- Consulted: Optics/Lens/Iris Maintainers
- Informed: Arca contributors

## Context and Problem Statement

Following the presentation decoupled host canvas recording architecture (Optics ADR-0127), the Optics graphics and UI engine release `v0.0.66` introduced foundational compound interaction patterns and input subsystem enhancements:

1. **Compound Interaction Patterns**:
   - `lens::patterns::popup_menu` and `menubar` (Optics ADR-Pattern-007): standard anchored popup menus with auto-flip, transient dismissal, right-aligned shortcut indicators, vector iconography, and built-in keyboard navigation (Up/Down arrows, Return activate, Escape dismiss).
   - `lens::patterns::DataGrid` (Optics ADR-Pattern-005): high-performance virtualized data tables with column resizing, sticky headers, and sorting.
   - `lens::patterns::TreeView` (Optics ADR-Pattern-006): hierarchical outliner trees with indentation, expansion chevrons, badges, and keyboard navigation.

2. **TextEdit Interaction Enhancements**:
   - Single-line submission semantics: pressing `Return` on single-line inputs emits `Response::submitted = true`.
   - Tab event propagation: pressing `Tab` when `accept_tab` is enabled emits `Response::tabbed = true`.
   - Native undo/redo history: multi-level history ring buffer with action coalescing and caret tracking (`Ctrl+Z`, `Ctrl+Shift+Z`, `Ctrl+Y`) directly in `lens_textedit`.

3. **Platform-Canonical Selectable Cursors**:
   - `lens_selectable` defaults to standard desktop arrow cursors (`pointer_cursor: false`), departing from web hyperlink pointer defaults and eliminating the need for post-hoc workarounds.

Arca must synchronize its workspace dependencies to `v0.0.66` and adopt these upstream design patterns.

## Decision Drivers

- Driver 1: **Robust Context Menu Interaction**: Handcrafted ad-hoc popup placement logic in `menus.rs` lacked keyboard navigation and uniform shortcut typography. Upstream `popup_menu` provides full accessibility and consistent transient lifecycle management.
- Driver 2: **Declarative Input Submissions**: Avoid routing key event inspection in global key handlers when individual single-line text edits (`textfield_ex`) natively report `Response::submitted`.
- Driver 3: **Native Undo/Redo**: Free inline renames, path location navigation, and chooser save names from raw buffer edits without custom undo stacks.
- Driver 4: **Desktop Cursor Conformance**: Cleanly align file listing and sidebar selectables with native desktop conventions without ad-hoc cursor resets.

## Architectural Decisions

### 1. Optics Dependency Floor Bump
All workspace dependencies for `iris`, `lens`, `lens-sys`, `flux`, `flux-sys`, `transit`, and `transit-sys` are bumped to `v0.0.66`.

### 2. Context Menu Pattern Adoption
`crates/arca-ui/src/menus.rs` adopts `iris::patterns::popup_menu` and `MenuItem`:
- `build_ctx_menu`: constructed with `MenuItem::new(...)` featuring crisp vector iconography (`icons::lens_id(...)`) and right-aligned shortcut typography (`.shortcut("Ctrl+C")`, `.shortcut("F2")`, etc.).
- `build_sidebar_menu`: constructed with `MenuItem` and `popup_menu`.
- Keyboard navigation (Up/Down arrow, Return, Escape) works out of the box in all context menus.

### 3. Direct TextEdit Submission Handling
- In `crates/arca-ui/src/content.rs`, inline renaming in both Grid view (`##grid-rename`) and List view (`##list-rename`) adopts `frame.textfield_ex(...)`. When `resp.submitted` is true, `app.commit_rename(frame)` is invoked directly on the widget response.
- In `crates/arca-ui/src/chooser.rs`, the save-name field (`##save-name`) uses `frame.textfield_ex(...)` and triggers file chooser acceptance immediately when `resp.submitted` is asserted.
- In `crates/arca-ui/src/toolbar.rs`, location editing consumes `loc_resp.submitted` and `loc_resp.tabbed`.

### 4. Native Desktop Cursor Semantics
With `lens_selectable` setting `LENS_CURSOR_DEFAULT` by default, selectable items throughout Arca naturally inherit desktop conventions without fighting web-style pointer overrides.

## Consequences

### Positive
- Fully accessible keyboard navigation in all context menus (Up/Down navigation, Return activation, Escape dismissal).
- Crisp vector icons alongside right-aligned shortcut key combinations in context menus.
- Immediate inline rename and save-as submission on Return key press directly from widget responses.
- Native multi-level undo/redo across all text editing inputs (`Ctrl+Z`, `Ctrl+Shift+Z`, `Ctrl+Y`).
- Reduced ad-hoc overlay code and cleaner architectural alignment with Optics compound patterns.

### Negative & Trade-offs
- Requires Optics `v0.0.66` or later as the baseline dependency floor.

## Rejected Alternatives

### 1. Continuing with handcrafted custom popover menus in `menus.rs`
- **Why considered**: Minimal immediate changes to existing menu rendering.
- **Why rejected**: Lacks native keyboard focus cycling (Up/Down arrow navigation), lacks standard shortcut layout alignment, and duplicates transient placement logic that Optics `popup_menu` now canonically standardizes.

### 2. Intercepting Return key in global key loop for rename commit
- **Why considered**: Avoided changing textfield calls in `content.rs`.
- **Why rejected**: Brittle focus-state coordination; widget-level `Response::submitted` allows the widget itself to signal single-line completion without cross-subsystem event snooping.

## Links

- [ADR 0001: Optics Stack Migration](0001-optics-stack-migration.md)
- [ADR 0004: Optics v0.0.60 Motion and Premultiplied Image Upload Alignment](0004-optics-v0.0.60-motion-and-image-pipeline-alignment.md)
- [ADR 0006: Optics v0.0.64 Segmented Controls, Orthogonal Icon Sizing, and Presentation Pacing Alignment](0006-optics-v0.0.64-segmented-controls-and-orthogonal-icon-metrics.md)
- Optics ADR-0127: Decoupled Host Canvas Recording and Pull-Driven Presentation Pacing
- Optics ADR-Pattern-005: High-Performance DataGrid Pattern
- Optics ADR-Pattern-006: Outliner Tree View Pattern
- Optics ADR-Pattern-007: Cascading Popup Menu and MenuBar Pattern
