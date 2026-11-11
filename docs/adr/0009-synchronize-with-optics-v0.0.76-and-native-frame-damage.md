---
id: ADR-0009
title: "Synchronize with Optics v0.0.76, Native Frame Damage, and Caret Phase Semantics"
status: accepted
date: 2026-11-10
scope: [ui/arca-ui, render, dev/standards]
superseded_by: null
negative_knowledge: true
---

# 0009. Synchronize with Optics v0.0.76, Native Frame Damage, and Caret Phase Semantics

- Status: Accepted
- Date: 2026-11-10
- Deciders: Arca Architecture Working Group
- Consulted: Optics/Lens/Iris Maintainers
- Amends: [ADR-0008](0008-synchronize-with-optics-v0.0.74-and-damage-quiescence.md)
- Extends: Optics ADR-0149 (damage-scoped windowed frames), ADR-0150 (SVG compiler), ADR-0151 (caret blink phase), ADR-0152 (ease termination), ADR-0155 (native frame damage ledger), ADR-0156 (Slang shaders)

---

## Context and Problem Statement

Between `v0.0.74` and `v0.0.76`, Optics moved damage ownership down into the
engine and completed the host-facing contracts Arca depends on:

1. **Damage-scoped windowed frames (Optics ADR-0149)**:
   - A windowed surface renders into one persistent content image and copies
     only the rectangles written since each swapchain image was last presented.
   - Lens now reports the pixels a widget actually changes, including border
     overhang, icon stroke halos, image outlines, connected tabs, scrolling
     viewports, restacked placement, and dismissed tooltips. A one-cell edit is
     no longer widened to the window.
   - Iris forwards the set to the compositor per rectangle instead of
     collapsing it to a bounding box.

2. **Native frame damage ledger (Optics ADR-0155)**:
   - `begin_frame` carries the frame's delta, `repaint_scope` returns the exact
     pixels the acquired image must paint, and `scanout_damage` reports what the
     display has not seen.
   - The surface owns one epoch history for offscreen and windowed images.
     Callers must not maintain a parallel per-image damage ledger, and an empty
     commit is a real no-change epoch.
   - Pairwise-disjoint damage decomposition and single-pass output blits
     (Optics ADR-0154) keep multi-rectangle updates legal and idempotent.

3. **Caret blink is a phase (Optics ADR-0151) and eases terminate (Optics ADR-0152)**:
   - Lens advances the blink clock once per build, reports damage only when the
     phase differs from the presented phase, and exposes the clock level
     separately from the flip edge.
   - Reduced motion is host policy: `set_caret_blink(period <= 0)` keeps the
     caret solid and silent. The default period is exported so a host never
     hard-codes it.
   - Exponential eases snap to their target, so focused fields and view
     switches can return to `IRIS_PACE_QUIESCE`.

4. **Host focus is label-scoped (bindings v0.0.76)**:
   - `clear_focus` moved from `Frame` to the host-facing `Ui`, and focus
     resolution is expressed against widget labels rather than numeric ids
     captured from an earlier build. Arca's cached `lens_id` fields were the
     stale-id pattern this contract removes.

5. **SVG and shader infrastructure changed underneath the stack**:
   - `libsvg` (Optics ADR-0150) compiles static SVG art, including the
     `<defs><g>` and truncated-container cases that icons rely on.
   - The entire renderer moved to native Slang shaders (Optics ADR-0156) with
     no GLSL compatibility path.
   - Pre-1.0 SONAMEs now carry the full version (`libflux.so.0.0.76`), so a
     binary built against another patch release fails at load time instead of
     reading a descriptor with a different layout.

Arca must synchronize to Optics `v0.0.76` and adopt these upstream ownership
boundaries rather than reimplementing them in the application.

---

## Decision Drivers

- Driver 1: **One Damage Authority**: The engine owns image epochs; Arca must
  not keep a second per-image damage history that can disagree with it.
- Driver 2: **Content-Accurate Localization**: Virtualized scrolling, row
  removal, modal transitions, caret flips, and border/icon fringes must damage
  the pixels they change and settle back to zero.
- Driver 3: **Stable Host Focus**: Focus targets must survive rebuilds without
  depending on a numeric id that a previous frame happened to assign.
- Driver 4: **Accessible Quiescence**: A focused field must not repaint every
  frame, and reduced motion must disable the blink clock entirely.
- Driver 5: **Fail-Fast Packaging**: A full-version SONAME is a deliberate
  boundary; local and installed builds must link the matching release without
  compatibility shims or a stale search path.

---

## Architectural Decisions

### 1. Optics Dependency Floor Bump

All workspace dependencies for `iris`, `lens`, `lens-sys`, `flux`, `flux-sys`,
`transit`, and `transit-sys` are bumped to `v0.0.76`. Arca consumes the
full-version SONAMEs and adds no compatibility layer for `v0.0.74` or earlier.

### 2. Native Damage Ledger Owned by Iris/Flux

Arca does not call `begin_frame`, `repaint_scope`, or `scanout_damage` directly
and does not maintain a per-image damage vector. Iris supplies Lens's damage
set to Flux and to the compositor. Arca's responsibility is limited to:

- reporting regional damage correctly through Lens (no manual widgets that
  paint outside their node without declaring the extent), and
- proving that the observed damage set stays localized and reaches quiescence.

The integration suite `crates/arca-ui/tests/damage_and_quiescence.rs` pins:

- departure damage when a trailing row is deleted,
- a one-wheel viewport change producing a bounded regional set,
- modal Quick Look opening and closing to zero damage,
- segmented-control animation termination, and
- caret blink flips producing damage only on the flip frame.

### 3. Label-Scoped Focus Application

Cached numeric widget ids (`location_id`, `filter_id`, `rename_id`) are deleted.
`UiApp` stores only a `FocusTarget` and applies it immediately after the target
widget is built, while Lens's id scope still describes that widget:

- `##location` receives select-all and focus in the toolbar,
- `##filter` receives focus in the toolbar, and
- `##grid-rename` / `##list-rename` receive the filename-stem selection and
  focus in the row/card that built them.

`Frame::clear_focus` has no shim; Arca calls the host-level focus primitive
through its existing narrow `lens_sys` bridge. Anonymous label resolution from
the root scope cannot address a nested field, which is why application happens
at the build site rather than at the end of the frame.

### 4. Caret Phase and Reduced-Motion Policy

Arca mirrors the host reduced-motion preference into Lens's caret clock:

- reduced motion → `lens_set_caret_blink(0.0)`, leaving a solid, silent caret;
- normal motion → `lens::DEFAULT_CARET_BLINK_PERIOD_S`, never a local copy of
  the period.

The policy is applied once per change, not once per frame. The integration test
drives a focused location field through both states and asserts that the blink
clock is live only in normal motion, that only phase flips produce damage, and
that a solid caret produces none.

### 5. SVG Icons Stay on the Lens Runtime Path

`assets/icons/*.svg` remain registered through `lens_icon_register_svg`; Arca
does not add a second SVG parser or pre-rasterize the set. The existing
`all_icon_assets_register` test now also guards the `libsvg` compiler migration:
every shipped glyph must parse and register before the UI can use it.

### 6. Transit RPATH Metadata Must Reach Build Scripts

The full-version SONAME exposed a packaging gap: `transit` is a safe wrapper
around the `links = "transit"` sys crate, so Cargo publishes
`DEP_TRANSIT_RPATHS` only to direct dependents. `arca-ui` and `arca` now depend
on `transit-sys` directly so their `build.rs` can relay the directory containing
`libtransit.so.0.0.76` into the test and binary runpaths. Installed builds
continue to resolve the same libraries through the system loader.

---

## Consequences

### Positive

- Frame damage has one authority: Iris/Flux. Arca can no longer drift from the
  engine's image-epoch history.
- Scrolling a virtualized listing, dismissing a modal, or deleting a row
  produces a bounded regional damage set and then returns to zero.
- Focus no longer depends on a widget id captured from an earlier build.
- A focused text field sleeps between caret flips, and reduced motion removes
  the blink clock entirely.
- The full-version SONAME turns an ABI mismatch into a load-time failure rather
  than silent descriptor corruption.

### Negative & Trade-offs

- Requires Optics `v0.0.76` or later as the baseline dependency floor; older
  installed libraries no longer satisfy the loader.
- Label focus must be applied at the widget's build site. A future widget moved
  into a different container must move its focus application with it.
- Direct `transit-sys` dependencies exist solely for Cargo links metadata; the
  comments in both manifests state that reason so they are not mistaken for
  unused crates.

---

## Rejected Alternatives & Negative Knowledge

### 1. Stay on `v0.0.74` until the next release

- **Why considered**: avoids the breaking focus and SONAME changes.
- **Why rejected**: `v0.0.74` lacks the native frame damage ledger and the
  damage-extent fixes for scrolling, borders, icons, and tooltips. Keeping the
  old floor would preserve full-window invalidation and a parallel damage model.

### 2. Keep caching numeric widget ids

- **Why considered**: the previous code worked and avoided resolving labels.
- **Why rejected**: ids are only meaningful inside the build that produced
  them. The v0.0.76 contract removes the `Frame`-level focus API precisely to
  stop hosts from carrying that stale identity across frames.

### 3. Maintain an Arca-side per-image damage ledger

- **Why considered**: gives the application control over partial presentation.
- **Why rejected**: it duplicates the engine's epoch history and recreates the
  first-use/abandonment divergence Optics ADR-0155 exists to remove.

### 4. Model the caret blink in Arca

- **Why considered**: lets the app decide when to request animation frames.
- **Why rejected**: the component that owns the visual state must own the clock
  and the damage edge. A second blink clock would reintroduce the level/edge
  confusion ADR-0151 removed.

### 5. Hard-code the default blink period

- **Why considered**: avoids depending on a newly exported constant.
- **Why rejected**: `period <= 0` is "off", so restoring the default requires a
  named period. A local copy would silently drift from Lens's platform cadence.

### 6. Add a `Frame::clear_focus` compatibility shim

- **Why considered**: one-line migration for existing call sites.
- **Why rejected**: focus is host state, not a draw operation. Keeping the old
  frame-scoped spelling would let future code keep treating focus as a widget
  property and bypass label-scoped resolution.

---

## Links

- [ADR 0001: Optics Stack Migration](0001-optics-stack-migration.md)
- [ADR 0008: Synchronize with Optics v0.0.74](0008-synchronize-with-optics-v0.0.74-and-damage-quiescence.md)
- [Optics Migration History](../dev/optics-migration.md)
- Optics ADR-0149: Damage-Scoped Frames on a Windowed Surface
- Optics ADR-0150: SVG Vector Document Compiler
- Optics ADR-0151: Caret Blink is a Phase, Not a Level
- Optics ADR-0152: An Ease Must Terminate
- Optics ADR-0154: Pairwise-Disjoint Damage and Idempotent Output Dither
- Optics ADR-0155: Frame Damage is a Native Epoch Ledger
- Optics ADR-0156: Slang-First Shader Architecture
