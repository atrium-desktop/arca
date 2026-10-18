---
id: ADR-0005
title: "0005. Fluid Adaptive Grid Architecture and Derived Typographic Metrics"
status: accepted
date: 2026-10-18
scope: [ui/arca-ui]
superseded_by: null
negative_knowledge: true
---

# 0005. Fluid Adaptive Grid Architecture and Derived Typographic Metrics

- Status: Accepted
- Date: 2026-10-18
- Deciders: Arca Architecture Working Group, Core Engineering
- Amends: [ADR-0003](0003-borderless-chrome-and-dense-hierarchy.md) (Section 5: Content Grid Geometry and Proportions)

---

## Context and Problem Statement

ADR-0003 introduced borderless chrome and tightened content grid proportions by locking content cards to fixed dimensions ($82 \times 78\,\text{px}$) with an $82\,\text{px}$ row pitch, $50\,\text{px}$ icons, and two-line centered labels.

In production use, this legacy configuration produced severe visual, ergonomic, and interaction defects:

1. **Row Collision / Text-Preview Overlap**: A $50\,\text{px}$ preview plus vertical padding ($6\,\text{px}$), inter-element gaps ($6\,\text{px}$), and two lines of $12.5\,\text{px}$ text ($\approx 36\text{--}38.5\,\text{px}$) requires $\approx 98\text{--}100\,\text{px}$ of vertical height. With card height hardcoded to $78\,\text{px}$ and row pitch to $82\,\text{px}$, the second line of text overflowed the card bounds by $>16\,\text{px}$, directly overlapping the thumbnail of the item in the subsequent row.
2. **Interaction Desynchronization**: Immediate-mode hit-testing uses the declared card container bounds ($78\,\text{px}$). Because the second line of text hung below this box and into the subsequent row's bounding box ($y \ge 82\,\text{px}$), clicking or dragging the bottom half of an item's label dispatched events to the card below it.
3. **Severe Horizontal Cramping**: An $82\,\text{px}$ card width yielded only $78\,\text{px}$ of usable text width, truncating standard filenames after 8–10 Latin characters or 4–5 CJK characters and forcing nearly 100% of filenames into two lines with heavy ellipsis.
4. **Unused Engine Seam**: `lens::patterns::virtual_grid_calc` calculates fluid allocated column widths (`allocated_col_w`) and exact virtual range slices, but Arca discarded these calculations in favor of static magic numbers, leaving ragged void margins on the right window border.

A clean break from static magic numbers was required to establish a self-healing, typographically sound, fluid grid architecture.

---

## Decision Drivers

- Driver 1: **Mathematical Non-Overlap Guarantee**: Ensure card height and row pitch can never collide under any valid font family, line-box metric, or multi-script text (Latin, CJK, diacritics).
- Driver 2: **Fluid Width Distribution**: Dynamically distribute available content width across columns to maximize readable filename length (104–144px range) and eliminate dead horizontal space.
- Driver 3: **Consistent Baseline Alignment**: Align all icons/thumbnails in a row at the top baseline regardless of whether sibling items have 1 or 2 lines of text or non-square aspect ratios.
- Driver 4: **Zero-Allocation Virtualization**: Maintain strict compliance with the 1 MiB Lens frame arena budget without introducing unbounded offscreen measurements.

---

## Architectural Decisions

### 1. Dynamically Derived Typographic Cell Height
Card height is no longer a static magic constant. It is derived per-frame from the active theme font's line box metrics:

$$\text{Card Height} = \left\lceil 2 \times \text{PAD\_V} + \text{ICON\_SIZE} + \text{GAP}_{\text{art-label}} + (N \times \text{Line Height}) + ((N - 1) \times \text{GAP}_{\text{line}}) \right\rceil$$

To guarantee headroom across multi-script typography (Latin ascenders/descenders, CJK em-squares, and diacritics), the line height measurement samples both Latin (`"Ag"`) and CJK (`"永"`):

$$\text{Line Height} = \max\left(\lceil \text{measure}(\text{"Ag"}).h \rceil, \lceil \text{measure}(\text{"永"}).h \rceil, \lceil \text{FONT\_SIZE} \times 1.4 \rceil\right)$$

At the resting default font ($12.5\,\text{px}$ Noto Sans), this resolves to $\text{Card Height} = 100\,\text{px}$ and $\text{Row Pitch} = 106\,\text{px}$ (with $\text{GRID\_GAP} = 6\,\text{px}$), maintaining $\ge 8\,\text{px}$ of guaranteed clearance between rows.

### 2. Fluid Adaptive Column Width Allocation
Cards transition from static $82\,\text{px}$ widths to fluid widths bounded between $\text{GRID\_MIN\_CARD\_WIDTH} = 104.0\,\text{px}$ and $\text{GRID\_MAX\_CARD\_WIDTH} = 144.0\,\text{px}$.

The active content width is resolved via `lens::patterns::virtual_grid_calc`, which computes:
- $\text{Columns} = \lfloor (\text{available} + \text{gap}) / (\text{min\_col\_width} + \text{gap}) \rfloor$
- $\text{Column Width} = (\text{available} - (\text{cols} - 1) \times \text{gap}) / \text{cols}$

Columns expand fluidly to absorb the available window width, eliminating the dead right margin and granting 35%–70% more horizontal space for filenames.

### 3. Top-Aligned Cross-Axis & Dedicated Art Slots
- Row containers and individual cards use `cross: Align::Start`.
- Each card encapsulates its preview art inside a fixed-height container ($\text{GRID\_ICON\_SIZE} = 50.0\,\text{px}$) with `cross: Align::Center`.
- Square album art, non-square thumbnails (16:9 widescreen or portrait), and system vector icons are centered within the $50\,\text{px}$ slot.
- Text labels across all cards in a row start at the exact same vertical baseline ($y = \text{PAD\_V} + \text{ICON\_SIZE} + \text{GAP}_{\text{art-label}}$). Single-line names leave clean padding at the bottom of the card; two-line names fill the allocated vertical budget.

### 4. Interactive Hit-Box Synchronization
The card container's hit box (`options.width` and `options.height`) matches the fluid width and derived height precisely. Mouse hover, click, right-click, and drag-and-drop targets are 1:1 synchronized with visible content.

---

## Normative Invariants Established

- `[INV-UI-GRID-01] Universal Multi-Script Clearance`: The derived grid card height MUST accommodate at least two full lines of CJK and Latin text plus icon art without exceeding `row_pitch - GRID_GAP`.
- `[INV-UI-GRID-02] Baseline Stability`: The preview thumbnail slot MUST have fixed height `GRID_ICON_SIZE` such that varying thumbnail aspect ratios do not shift text label baselines within the same row.
- `[INV-UI-GRID-03] Fluid Arena Compliance`: Grid layout MUST remain strictly virtualized via `virtual_rows` and `virtual_grid_calc`, with zero dynamic allocation of offscreen items, guaranteeing completion well within the 1 MiB frame arena budget.

---

## Consequences

### Positive
- **Zero Row Collisions**: File names never overlap picture previews under any font or language.
- **Accurate Hit-Testing**: Mouse clicks and drag initiation on multi-line labels correctly target the item being clicked.
- **Human-Readable Labels**: Increased width ($104\text{--}144\,\text{px}$) substantially reduces premature truncation and multi-line breaks for standard desktop filenames.
- **Fluid Polish**: Window resizing smoothly adjusts card widths without ragged right-side void space.
- **Self-Healing Geometry**: If font sizes or theme metrics change, card height automatically scales to maintain non-overlap invariants.

### Negative & Trade-offs
- Slightly reduced icon density compared to the overly aggressive $82\,\text{px}$ grid (e.g., ~7–8 columns on a 1080p display instead of 11 columns).
- Mitigation: Readability and click accuracy take precedence; zoom scaling tiers can be added in future work.

---

## Rejected Alternatives & Negative Knowledge

### 1. Variable Dynamic Row Heights (Masonry/Flow Layout)
- **Why considered**: Allowing each row to shrink to 1 line if all items in that row have short names.
- **Why rejected**: Violates spatial 2D arrow-key navigation (vertical jumping), causes visual cadence jitter across rows, and requires $O(N)$ off-screen layout measurements that exceed the 1 MiB Lens frame arena budget for directories with 100,000+ files.

### 2. Retaining Static Constants with a Higher Magic Number (e.g., 98px / 102px fixed)
- **Why considered**: Simplest static patch requiring no runtime font measurement.
- **Why rejected**: Brittle. Under non-standard fonts, CJK fallback glyphs, or user accessibility font scaling, fixed magic numbers inevitably break again. Runtime derivation from active `Frame` text metrics is required for architectural robustness.
