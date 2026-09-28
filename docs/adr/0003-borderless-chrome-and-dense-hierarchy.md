---
id: ADR-0003
title: "0003. Borderless Surfaces, Subtle Controls, and Visual Density Hierarchy"
status: accepted
date: 2026-09-10
---

# 0003. Borderless Surfaces, Subtle Controls, and Visual Density Hierarchy

- Status: Accepted
- Date: 2026-09-10
- Deciders: Ming Li, Arca UI Architecture Team
- Consulted: Tessera Design System Team, Optics/Lens Maintainers
- Informed: Arca contributors

## Context and Problem Statement

Arca's visual language was conceived to mirror the Tessera design system (`tessera-design`), running on the Optics/Iris/Lens stack. However, the initial UI implementation suffered from several visual defects and low information density:

1. **"Border Soup" Chrome**: Toolbar controls (navigation arrows, refresh, visibility toggle, settings) defaulted to standard bordered button widgets (`LENS_BUTTON_DEFAULT`), encasing every action in an explicit circular 1px stroke outline.
2. **Disconnected Input Fields**: The location and search/filter inputs used prominent outline rings with glyph icons placed loosely outside the input perimeter rather than unified inside recessed surface containers.
3. **Exaggerated Navigation Pitch**: Sidebar items rendered with standard 12px vertical padding and 12px pill radii, creating oversized 38px capsules with 40px+ baseline pitch, severely limiting visible directory bookmarks.
4. **Sparse Content Grid**: Content cards were locked to 136px width by 120px height with 48px icons and 10px margins, leaving ~100px of empty horizontal dead space between folder glyphs and ~50px of vertical gap between rows.

These visual artifacts produced a cluttered yet under-dense experience uncharacteristic of modern high-productivity desktop operating systems.

## Decision Drivers

- Driver 1: **Fluid, Borderless Visual Language**: Align Arca with modern desktop paradigms (Tessera Liquid Glass & Analytic Surfaces, GNOME Adwaita 45+, macOS Sonoma) where chrome is borderless by default and content takes precedence.
- Driver 2: **Intentional Focus & Hover Affordances**: Controls remain borderless and transparent at rest; interactive affordance is conveyed on hover (`bg_hover`) and active press (`bg_pressed`), without visual clutter when idle.
- Driver 3: **Surface Elevation over Outline Strokes**: Prominent interactive containers (location bar, search/filter) use recessed background fill surfaces (`tones.card` / `tones.window` inset) rather than harsh outlines.
- Driver 4: **Ergonomic Information Density**: Balance readability and touch target size for desktop mice/touchpads: 28–30px sidebar rows with concentric 6–8px corner radii, and compact content grid proportions that eliminate excessive void space.

## Considered Options

- Option 1: Retain hard outlines and borders, merely reducing stroke width to 0.5px and tweaking gap constants.
- Option 2: Full re-skin using custom raw draw calls in every widget site in `arca-ui`.
- Option 3: Standardize on Optics/Lens `LENS_BUTTON_SUBTLE` for all toolbar actions, encapsulate inputs in recessed surface containers with integrated leading icons, compact sidebar items to 30px height / 6px radius, and tune grid card geometry and virtual grid calculations to tight proportions (96–112px pitch with balanced art padding).

## Decision Outcome

Chosen option: **Option 3**, because it leverages existing Optics/Lens capabilities (`LENS_BUTTON_SUBTLE`, style cascades, and flex containers) cleanly without technical debt, adheres strictly to Tessera design tokens, and resolves the visual density and visual hierarchy problems across the entire application.

### Implementation Specifics

1. **Toolbar Actions (Ghost/Subtle Buttons)**:
   - All navigation buttons (`ArrowLeft`, `ArrowRight`, `ArrowUp`, `RefreshCw`), toggles (`EyeOff`/`Eye`), and auxiliary buttons (`Settings`) use `LENS_BUTTON_SUBTLE`.
   - Rest state: borderless (`border_width = 0`), transparent background.
   - Hover state: smooth `bg_hover` background tint with concentric corner radius.
   - Active state: `bg_pressed` feedback.

2. **Unified Surface Containers for Inputs**:
   - Location bar and search bar are rendered as recessed containers using soft surface fill colors (`tones.elevated` / `tones.card` or semi-transparent background wash) with gentle hairline borders or pure borderless fills.
   - Glyphs (folder icon for path, magnifying glass for filter) are integrated inside the container margins as leading icons.

3. **Sidebar Density and Proportions**:
   - Sidebar item height tightened from 38px to 30px with 4px vertical padding and 6px horizontal padding.
   - Selection capsule corner radius refined from 12px to 7px (conforming to `tessera-design` `Radii::menu_item`).
   - Section labels tightened with proportional padding.

4. **Removal of Redundant Content Heading**:
   - The 40px content heading bar displaying directory name, item count, and redundant sort link (`ming 20 items Name`) has been removed entirely.
   - Directory identity is already surfaced in the tab bar and location input; item counts and volume capacity are continuously reported by the persistent status bar.
   - This reclaims 40px of vertical workspace and removes triplicated information.

5. **Borderless Content Grid & Pure Color State Feedback**:
   - Card dimensions adjusted to 82×78px, with 50px enlarged icons and 12.5px text.
   - Gap between cards set to 4px with an 82px row pitch.
   - Full-card background boxes on hover and click are eliminated completely (`bg: Color::TRANSPARENT`).
   - Interaction states transition via color rather than geometric bounding boxes:
     - Resting state: soft off-white text (`rgba(215, 222, 235, 255)`).
     - Hover state: high-contrast pure white text (`#FFFFFF`) with zero background box.
     - Selected state: vibrant accent text (`theme.accent()`) with zero background box.
   - Virtual grid calculation updated to adapt column count dynamically with compact thresholds.

### Positive Consequences

- Clean, distraction-free window chrome where content takes center stage.
- Significantly increased bookmark and file count visibility without scrolling.
- Consistent interaction feedback: hover highlights appear smoothly under cursor movement.
- Eliminates visual noise across light and dark theme variations.

### Negative Consequences

- Tighter grid width allows slightly fewer characters before label wrapping/ellipsizing.
- Mitigation: Two-line wrapped centered labels with character-boundary fallback preservation ensure names remain legible.

## Links

- [ADR 0001: Optics Stack Migration](0001-optics-stack-migration.md)
- [Tessera Design Foundations: Shape and Border](../../tessera-dev/docs/dev/design/foundations/shape-and-border.md)
- [Tessera Design Foundations: Space and Layout](../../tessera-dev/docs/dev/design/foundations/spacing-and-layout.md)
