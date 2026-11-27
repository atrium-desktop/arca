---
id: ADR-0022
title: "Grid as the Default View and Centred Miller Columns"
status: accepted
date: 2026-11-25
scope: [engine/config, engine/state, presentation/ui]
superseded_by: null
negative_knowledge: true
---

# 0022. Grid as the Default View and Centred Miller Columns

- Status: Accepted
- Date: 2026-11-25
- Deciders: Arca Architecture Working Group
- Consulted: Optics UI Maintainers
- References: [View Modes Reference](../reference/view-modes.md), [ADR-0005](0005-fluid-adaptive-grid-and-typographic-metrics.md), [ADR-0020](0020-asynchronous-directory-navigation.md)

---

## Context and Problem Statement

Two view-mode behaviours worked against users:

1. **List was the default.** New users landed in a dense metadata table. The
   thumbnail pipeline (album covers, photos) is only visible in Grid, so the
   feature that makes media folders recognisable was hidden behind a toggle.
2. **Miller put the active directory in the rightmost column.** The model held
   the active directory and up to three ancestors, and the UI showed the last
   ones that fit. Every navigation therefore moved the user's focus: entering a
   folder pushed the old columns left and the new active column appeared at the
   right edge. Nothing showed what was *inside* the selected entry, the main
   reason to use Miller columns. On wide windows, up to three ancestor
   directories were read synchronously after every navigation (the residual
   noted in the interaction-latency report).

---

## Decision Drivers

- Driver 1: **First impression.** A new user's first folder should show artwork
  where it exists.
- Driver 2: **Stable focus.** The column the user works in must not move when
  they navigate.
- Driver 3: **Look ahead.** Miller should show the contents of the entry under
  the cursor, as column browsers on other desktops do.
- Driver 4: **No UI-thread I/O per keystroke** (ADR-0020). The preview target
  changes on every `Up`/`Down`, so the preview cannot read inline.
- Driver 5: **Respect existing choices.** Users who saved a mode keep it.

---

## Decisions

### 1. Grid is the default view

`ViewMode::default()` is `Grid`. It applies when `arca.conf` has no `view` key or
an unrecognised value. A saved `view = list` or `view = miller` is still honoured.
Before, an unrecognised value fell back to List; it now falls back to the default.

### 2. Miller centres the active directory: parent · active · preview

- **Engine model.** The column model holds at most two columns: the parent (when
  one exists) and the active directory. Only the parent costs an extra
  `read_dir`, down from up to three ancestor reads.
- **Preview column.** The right column is derived from the cursor:
  - a directory shows its listing;
  - a file shows a details card (artwork, name, MIME type, size, modified);
  - no selection leaves it blank.
- **Fixed slots.** The UI lays out three slots: Parent | Active | Preview.
  - At the filesystem root the parent slot is rendered blank rather than
    dropped, so the active column never shifts.
  - When three 220 px columns do not fit, only the active column is shown.
    Exactly two columns are never shown, because the active directory could not
    then be centred.
- **Keys keep their meaning.** `Left` moves to the parent and `Right` enters the
  selected directory. The new active directory moves into the centre; the user's
  eye stays put.

### 3. The preview reads off-thread with ADR-0020's contract

`arca_engine::miller_preview::PreviewLoader` gives one named worker per request
and a generation counter, so only the newest request publishes.

- **Synchronous mode.** Tests and embedders without a frame loop read inline.
- **Cache key.** A result is reused while its key is unchanged: path,
  hidden-file visibility, sort key, and sort direction.
- **Engine API.** `AppState::request_miller_preview`, `poll_miller_preview`,
  `is_loading_miller_preview`, and `miller_preview`. The UI requests the preview
  during the Miller build, after key handling has moved the cursor, and keeps
  painting while a read is in flight.

---

## Consequences

### Positive

- New users see thumbnails immediately.
- The Miller focus column is fixed. The preview answers "what's in there?"
  without navigating.
- Miller does fewer synchronous reads per navigation: one (the parent) instead
  of up to three.

### Negative & Trade-offs

- **Fewer ancestors on screen.** Deep ancestry (grandparents and above) is no
  longer visible as columns; the breadcrumb bar shows the full path.
- **Preview freshness.** The preview listing is not watched for filesystem
  events; it refreshes when the cursor moves onto it again or when sort or
  hidden-file options change. Watching it would re-arm inotify on every cursor
  move.
- **Tests and the default.** Tests that relied on List as the default now pin
  their mode explicitly (`override_view_mode(ViewMode::List)`).

---

## Rejected Alternatives & Negative Knowledge

### 1. Keep the ancestor chain and scroll the active column to the centre

- **Why rejected**: centring needs as many columns right of the active one as
  left of it. Without a preview, the right half of the window would be empty
  space. With one, the extra ancestors still cost synchronous reads, and they
  repeat information the breadcrumb bar already shows.

### 2. Show two columns (parent · active) on medium widths

- **Why rejected**: the active column would sit on the right again at exactly
  the widths where users are most likely to notice the jump. One centred column
  is the narrow fallback.

### 3. Read the preview inline when the cursor moves

- **Why rejected**: violates ADR-0020. Holding `Down` through a column of large
  or network-mounted folders would stall a frame per step.

### 4. Change the default to Grid by rewriting existing configs

- **Why rejected**: a saved `view` is the user's explicit choice. Only an absent
  or invalid value takes the new default.
