---
id: ADR-0008
title: "Synchronize with Optics v0.0.74, Regional Damage Tracking, and Quiescent Presentation"
status: accepted
date: 2026-11-06
scope: [ui/arca-ui, render, dev/standards]
superseded_by: null
negative_knowledge: true
---

# 0008. Synchronize with Optics v0.0.74, Regional Damage Tracking, and Quiescent Presentation

- Status: Accepted
- Date: 2026-11-06
- Deciders: Arca Architecture Working Group
- Consulted: Optics/Lens/Iris Maintainers
- Informed: Arca contributors
- Amends: [ADR-0006](0006-optics-v0.0.64-segmented-controls-and-orthogonal-icon-metrics.md), [ADR-0007](0007-optics-v0.0.66-compound-patterns-and-textedit-interaction-alignment.md)
- Extends: Optics ADR-0143 (regional damage rect lists), Optics ADR-0144/ADR-0149 (windowed frame damage scoping), Optics ADR-0151 (caret blink phase), Optics ADR-0152 (ease termination), Optics ADR-0153 (library diagnostics)

---

## Context and Problem Statement

Between `v0.0.66` and `v0.0.74`, the upstream Optics rendering and UI engine underwent fundamental improvements across damage calculation, frame lifecycle, presentation pacing, and text interaction:

1. **Regional Damage Rect Lists and Paint-Bounds Localization (Optics ADR-0143, ADR-0149)**:
   - Upstream replaced the scalar bounding box (`lens_damage_rect`, deleted without shim) with a bounded regional rectangle list contract (`lens_damage_rects`, capped at 64 disjoint rectangles).
   - Node damage is localized to actual paint bounds (`lensi_text_bounds`), so trailing rows, flow nodes, and centred labels no longer over-invalidate the full container.
   - Detached/departing nodes (`LENS_NODE_LEAVING`) emit damage for their vacated areas upon removal, eliminating stranded pixels when items are deleted from listings or popups are dismissed.
   - Surface presentation tracks write-epoch history so partial passes onto windowed swapchain images remain divergence-free.

2. **Caret Clock Ownership & Presentation Quiescence (Optics ADR-0151)**:
   - Lens owns the caret blink phase, separating clock ticks from damage edges. A focused text field no longer generates perpetual full-window chrome damage, allowing idle applications to reach `IRIS_PACE_QUIESCE` in Wayland compositors.

3. **Ease Termination and Settle Snapping (Optics ADR-0152)**:
   - Exponential easing (`lensi_approach`) snaps precisely to the target value when within threshold instead of drifting infinitely with floating-point epsilon. This ensures animated widgets (e.g. `SegmentedControl` view switcher) cleanly terminate (`ui.anim_pending() == false`) and enter quiescence.

4. **Host-Controlled Caret & Selection for TextFields**:
   - `Frame::textfield_set_caret`, `Frame::textedit_set_caret`, and `Frame::textedit_set_selection` allow the application to position carets and pre-select text ranges (e.g. selecting the filename stem on rename, selecting all on location focus, or advancing to the end of a Tab-completed directory).

5. **Diagnostic Error Preservation (Optics ADR-0153)**:
   - `lens::Error` and `flux::Error` preserve rich C runtime diagnostics (`FluxDiagnostic`) captured at the failure site rather than flattening errors into bare numerical result codes.

Arca must synchronize its workspace dependencies to Optics `v0.0.74` and adopt these upstream design patterns.

---

## Decision Drivers

- Driver 1: **Accurate Invalidation & Stale Pixel Prevention**: Deleting files or switching views must emit localized damage covering vacated areas without relying on expensive full-window repaints.
- Driver 2: **Quiescent CPU/GPU Efficiency**: A settled file manager window must sleep in `poll()` with zero damage rects and no active animation frames, preserving battery and compositor resources.
- Driver 3: **Native Text Selection Ergonomics**: Location bar navigation (Ctrl+L) and inline file renaming (F2) must follow native desktop conventions (select-all for paths, stem-only selection for filenames).
- Driver 4: **Standards & Build Unification**: Modernize build documentation to unified `build-and-run.md` following ecosystem standards (Optics ADR-0125).

---

## Architectural Decisions

### 1. Optics Dependency Floor Bump
All workspace dependencies for `iris`, `lens`, `lens-sys`, `flux`, `flux-sys`, `transit`, and `transit-sys` are bumped to `v0.0.74`.

### 2. Adoption of Regional Damage Verification & Quiescence Tests
Arca adopts `Ui::damage_rects` to verify that:
- Settled directory listings emit 0 damage rects and report `!ui.anim_pending()`.
- Deleting items (trailing rows) emits localized departure damage (via `LENS_NODE_LEAVING`) within the 64-rect contract and subsequently settles to quiescence.
- Modal Quick Look preview transitions cleanly into and out of quiescence.
- View-mode `SegmentedControl` switches terminate animation within bounded frames (ADR-0152).

These guarantees are pinned in the automated test suite `crates/arca-ui/tests/damage_and_quiescence.rs`.

### 3. Host-Controlled Selection for Location and Inline Rename
Arca integrates `Frame::textedit_set_selection` and `Frame::textfield_set_caret`:
- **Location Focus (`Ctrl+L`)**: Automatically pre-selects the entire location buffer (`0..u32::MAX`) so typing immediately replaces the path or navigation keys adjust the position.
- **Tab Completion**: Positions the caret at the end of the completed candidate path via `frame.textfield_set_caret("##location", len)`.
- **Inline Rename (`F2` / Context Menu)**: Pre-selects the filename stem (excluding the file extension) for regular files, preserving standard extensions from accidental clobbering while selecting the entire label for folders.

### 4. Build & Run Documentation Consolidation
Conforming to Optics ADR-0125 and sibling project conventions, `manual-testing.md` is consolidated into `docs/dev/build-and-run.md`, covering compilation, sandbox isolation, standalone picker, portal prompter, and headless testing.

---

## Consequences

### Positive
- Zero CPU/GPU wakeups during static idle; window enters compositor quiescence.
- Vacated row pixels are cleanly damaged and cleared upon file deletion without full-display invalidation.
- File renaming and path entry provide standard desktop selection ergonomics out of the box.
- Standardized documentation structure aligned with sibling projects.

### Negative & Trade-offs
- Requires Optics `v0.0.74` or later as the baseline dependency floor.

---

## Rejected Alternatives

### 1. Manual `request_animation_frame()` loops for idle views
- **Why rejected**: Perpetually wakes the event loop and defeats compositor presentation pacing, generating unnecessary compositor Render/3D load.

### 2. Manual string splitting and cursor manipulation outside text edit widgets
- **Why rejected**: Optics now provides first-class host caret/selection primitives (`textedit_set_selection` / `textfield_set_caret`) directly in the widget layer.

---

## Links

- [ADR 0001: Optics Stack Migration](0001-optics-stack-migration.md)
- [ADR 0006: Optics v0.0.64 Segmented Controls](0006-optics-v0.0.64-segmented-controls-and-orthogonal-icon-metrics.md)
- [ADR 0007: Optics v0.0.66 Compound Patterns](0007-optics-v0.0.66-compound-patterns-and-textedit-interaction-alignment.md)
- Optics ADR-0143: Regional Damage Rect Lists and Paint-Bounds Localization
- Optics ADR-0149: Damage-Scoped Frames on a Windowed Surface
- Optics ADR-0151: Caret Blink is a Phase, Not a Level
- Optics ADR-0152: An Ease Must Terminate
- Optics ADR-0153: A Binding Must Carry the Library's Diagnostic
