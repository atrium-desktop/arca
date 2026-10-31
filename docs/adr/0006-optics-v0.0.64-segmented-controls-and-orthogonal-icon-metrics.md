---
id: ADR-0006
title: "0006. Optics v0.0.64 Segmented Controls, Orthogonal Icon Sizing, and Presentation Pacing Alignment"
status: accepted
date: 2026-10-31
scope: [ui/arca-ui]
superseded_by: null
negative_knowledge: true
---

# 0006. Optics v0.0.64 Segmented Controls, Orthogonal Icon Sizing, and Presentation Pacing Alignment

- Status: Accepted
- Date: 2026-10-31
- Deciders: Ming Li, Arca UI Architecture Working Group
- Consulted: Optics/Lens/Iris Maintainers, Tessera Design System Team
- Informed: Arca contributors

## Context and Problem Statement

Arca relies on the Optics graphics and UI engine (`flux`, `lens`, `iris`, `transit`). Between Optics `v0.0.60` and `v0.0.64`, significant architectural advancements were landed upstream:

1. **Orthogonal Icon Sizing and Paint Run Preservation (Optics ADR-0111, ADR-0112)**:
   - In previous builds, list and sidebar selectable row icons were rigidly coupled to label typography (`icon_size = font_size = 14px`). Forcing 18–20px vector glyphs into a 14px box placed fine geometry on fractional non-integer pixel coordinates, producing visible blurriness on standard DPI displays.
   - Mixed stroke and fill SVGs sharing `currentColor` previously risked collapsing the paint run table during runtime registration.
   - Optics ADR-0111 introduced independent `icon_size` tokens on `lens_theme` and `lens_style`, alongside `Frame::selectable_icon_sized`. Optics ADR-0112 introduced 8×8 supersampled device-resolution icon coverage masks blitted with 1:1 pixel snapping.

2. **Native Segmented Controls & Refined Subtle Button Radii (Optics commit 75925ea)**:
   - Native segmented controls (`SegmentedControl`, `SegmentedItem`, `Frame::segmented`) were exposed directly to the Rust bindings and re-exported at root level in `iris`.
   - Subtle buttons (`LENS_BUTTON_SUBTLE`) acquired automated proportional corner radius refinement (`fmaxf(h * 0.25f, 4.0f)`), aligning toolbar buttons directly with Tessera design tokens without manual style overrides.

3. **Display-Driven Presentation Pacing & Subsurface Compositing (Optics ADR-0121 - ADR-0124)**:
   - Presentation loop migrated from client-side 60 Hz timer polling to hardware display vertical blanking synchronization (Wayland `wl_surface.frame` / `wp_presentation_feedback`, high-refresh rate 120/144 Hz display discovery).
   - Multi-layer compositing and `lens_damage_rect` incremental presentation support reduce redraw overhead for localized UI updates.

Arca must synchronize its workspace dependencies to `v0.0.64` and adopt these upstream design patterns.

## Decision Drivers

- Driver 1: **Vector Icon Clarity**: Typography (14px) and navigation icons (18px) must follow independent visual weight scales to ensure integer pixel alignment and crisp rendering.
- Driver 2: **Unified Optics Import Boundary**: Consume first-class patterns (`SegmentedControl`, `SegmentedItem`) from `iris` root exports rather than private deep module paths.
- Driver 3: **Consistent Desktop Metrics**: Leverage upstream subtle button corner refinements and display-driven hardware pacing without custom in-tree workarounds.
- Driver 4: **Global Lockstep Versioning**: Synchronize workspace manifests to Optics `v0.0.64`.

## Considered Options

- Option 1: Bump version tags to `v0.0.64` in `Cargo.toml` without adopting new widget or styling APIs.
- Option 2: Full adoption of Optics `v0.0.64`:
  - Update `branded_theme_with_accent` with `.with_icon_size(18.0)`.
  - Adopt `Frame::selectable_icon_sized` in `arca-ui::icons` for sidebar and menu rows.
  - Re-export and import `SegmentedControl` and `SegmentedItem` directly from `iris`.
  - Remove dead pre-segmented control helpers.
  - Rely on upstream display-pacing and subtle button corner radii.

## Decision Outcome

Chosen option: **Option 2**, because it fully aligns Arca with Optics' canonical C23 and Rust design paradigms, eliminates subpixel icon blur, and streamlines the UI chrome.

### Implementation Specifics

1. **Optics Dependency Bump to `v0.0.64`**:
   - `Cargo.toml` updates `iris`, `lens`, `lens-sys`, `flux`, `flux-sys`, `transit`, `transit-sys` to `v0.0.64`.
   - Maintained zero-friction local development worktree compatibility via `.cargo/optics-local.toml`.

2. **Orthogonal Icon Sizing in Theme & Selectables**:
   - `crates/arca-ui/src/theme.rs`: `branded_theme_with_accent` configures `.with_icon_size(18.0)`.
   - `crates/arca-ui/src/icons.rs`: `selectable_icon` and `selectable_icon_sized` adopt `frame.selectable_icon_sized(label, lens_id(id), selected, icon_size)`.

3. **Segmented Control & Subtle Button Refinement**:
   - `crates/arca-ui/src/toolbar.rs`: Imports `SegmentedControl` and `SegmentedItem` from `iris`.
   - `crates/arca-ui/src/icons.rs`: Removed obsolete `icon_button_active_rounded`.
   - Toolbar subtle buttons inherit the refined corner radii from Optics button skin.

4. **Robust Inotify Test Handling**:
   - Unit tests handling filesystem watching (`watch.rs`, `state.rs`, `lib.rs`) gracefully skip when the host system kernel inotify instance budget is exhausted, preventing false CI failures.

### Positive Consequences

- Sidebar and menu icons are rendered at 18px with pixel snapping, avoiding subpixel smearing against 14px text.
- SVGs with mixed fill and stroke retain full color fidelity under `currentColor`.
- Window animations and transitions naturally lock to the host monitor's native refresh rate (e.g. 120 Hz).
- Cleaner and more idiomatic Optics integration surface.

### Negative Consequences

- Requires Optics `v0.0.64` or later as the baseline dependency floor.

## Rejected Alternatives

### 1. Retaining coupled 14px icon sizing
- **Why considered**: Avoids modifying theme metrics.
- **Why rejected**: Violates visual weight requirements; 24px viewBox vector glyphs compressed to 14px alias heavily and look disproportionately small next to 14px labels.

### 2. Manual corner radius style injection on every subtle button
- **Why considered**: Allows custom per-button curvature overrides.
- **Why rejected**: Incurs redundant styling overhead when Optics' refined subtle button skin already guarantees canonical 25% height bounds conforming to Tessera standards.

## Links

- [ADR 0001: Optics Stack Migration](0001-optics-stack-migration.md)
- [ADR 0004: Optics v0.0.60 Motion and Premultiplied Image Upload Alignment](0004-optics-v0.0.60-motion-and-image-pipeline-alignment.md)
- [ADR 0005: Fluid Adaptive Grid Architecture and Derived Typographic Metrics](0005-fluid-adaptive-grid-and-typographic-metrics.md)
- Optics ADR-0111: Lens Runtime SVG Mixed Paint Runs and Orthogonal Icon Sizing
- Optics ADR-0112: Device-Resolution Icon Coverage Masks
- Optics ADR-0122: Compositor-Driven Presentation Pacing
- Optics ADR-0123: Layered Compositing, Hardware Subsurfaces, and Damage Tracking
- Optics ADR-0124: Decoupled Dual-Thread UI and Presentation Engine
