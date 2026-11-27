# View Modes Reference

Arca presents a directory in one of three view modes: **Grid**, **List**, and
**Miller columns**. This page specifies what each mode shows, how it is laid out,
how it responds to input, and what it costs. The rationale behind the default
mode and the Miller layout is recorded in ADR-0022.

---

## Summary

| Mode | `view =` | CLI flag | Shows | Best for |
|------|----------|----------|-------|----------|
| **Grid** (default) | `grid` | `--grid` | Square tiles: artwork or type icon, name below | Photos, music, any folder recognised by its artwork |
| **List** | `list` | `--list` | One row per entry with Name, Size, Modified | Scanning and sorting metadata |
| **Miller** | `miller` | `--miller` | Parent, active directory (centre), preview | Walking a directory tree while keeping your place |

All three modes share one model:

- **Same selection.** The cursor, multi-selection, and range anchor are stored by
  entry name, so switching modes keeps the same files selected.
- **Same listing.** Sorting, hidden-file visibility, and the filter field apply to
  every mode.
- **Same operations.** Copy, cut, paste, rename, trash, drag-and-drop, the context
  menu, and Quick Look act on the selection the same way in every mode.
- **Virtualized.** Only rows or tiles inside the viewport (plus two rows of
  overscan at each edge) are built and drawn, so a directory of tens of thousands
  of entries costs the same per frame as a small one.

---

## Choosing a Mode

| Method | Persists? |
|--------|-----------|
| Toolbar view switcher (Grid · List · Miller) | Yes, written to `arca.conf` |
| `view = grid \| list \| miller` in `~/.config/arca/arca.conf` | Yes |
| `arca --grid`, `--list`, or `--miller` at launch | No, this window only |

- **Default.** When `view` is missing or has an unrecognised value, Arca uses
  **Grid**. A saved `view = list` or `view = miller` is honoured unchanged.
- **Scope.** The mode is global: every tab uses the active mode.

---

## Grid

Tiles flow left to right and wrap into rows.

### Layout

| Property | Value |
|----------|-------|
| Tile width | Fluid, 104–144 px: as many columns as fit the content width, then each tile's width is clamped to that range |
| Artwork slot | 50 px square, top-aligned |
| Name | 12.5 px, up to 2 lines, then ellipsized. Line height is measured from the active font (Latin and CJK) so two-line names never overlap the next row. |
| Gap | 6 px between tiles and rows |

### Artwork

- **Thumbnails.** When thumbnails are on, images (PNG, JPEG) and audio with
  embedded covers (FLAC, MP3) show their artwork; every other entry shows its
  type icon.
- **Loading.** Thumbnails are decoded off the UI thread, only for tiles in view,
  and newest request first. They are cached in the shared desktop thumbnail cache
  (see [Thumbnail Extraction and Caching Pipeline](../explanation/thumbnail-pipeline.md)).
- **Selection.** A selected tile's artwork is dimmed.

### Keyboard

| Key | Action |
|-----|--------|
| `Left` / `Right` | Previous / next tile |
| `Up` / `Down` | Same column, previous / next row |
| `Enter` | Open the selected entry |

---

## List

One row per visible entry.

### Layout

| Property | Value |
|----------|-------|
| Row height | 42 px, plus a 2 px gap |
| Columns | Name (type icon, then name; fills the remaining width), Size (86 px), Modified (124 px) |
| Header | Click a column header to sort by it; click again to reverse the order. The active column shows a direction chevron. |
| Rows | The selection is highlighted |

### Keyboard

| Key | Action |
|-----|--------|
| `Up` / `Down` | Previous / next row (`Shift` extends the selection) |
| `Enter` | Open the selected entry |

---

## Miller Columns

The active directory always sits in the **centre** column. Its parent is on the
left; the right column previews whatever the cursor points at. Navigation moves
the content of the columns, not your focus: the column you are working in never
changes position.

```text
┌──────────────┬──────────────────┬──────────────────────┐
│ Parent       │ Active (centre)  │ Preview              │
│              │                  │                      │
│  Documents   │  Projects      ▸ │  arca/               │
│▶ Music      ▸│▶ Albums        ▸ │  ├─ 2019 Live        │
│  Pictures   ▸│  Singles       ▸ │  └─ cover.png        │
│              │  notes.txt       │                      │
└──────────────┴──────────────────┴──────────────────────┘
   ▶ = selected     ▸ = navigable directory
```

### Columns

| Slot | Content |
|------|---------|
| **Parent** (left) | Listing of the active directory's parent, with the active directory highlighted and scrolled into view. **Blank** at the filesystem root, so the centre column never shifts. |
| **Active** (centre) | The current directory. It is the only column that honours the filter field, accepts right-click and multi-selection, and owns the cursor. Its header uses the selected tone and a larger title. |
| **Preview** (right) | Depends on the cursor: |
| | • **Directory**: its listing, scrolled to the top. Shows *Loading…* while the read is in flight, *Empty folder* when it has no entries, or the read error. |
| | • **File**: a details card with its artwork (thumbnail when available, otherwise the type icon, 128 px), name, MIME type, size, and modification time. |
| | • **Nothing selected**: blank. |

### Width

| Content area | Layout |
|--------------|--------|
| ≥ 676 px (three 220 px columns plus two 8 px gaps) | Parent · Active · Preview, each column taking an equal share of the width |
| < 676 px | Active only |

Miller never shows exactly two columns: with an even count the active directory
could not be in the middle.

### Input

| Input | Action |
|-------|--------|
| `Up` / `Down` | Move the cursor in the centre column; the preview follows |
| `Right` | Enter the selected directory: it moves to the centre and the old active directory becomes the parent. On a file, opens it. |
| `Left` | Go to the parent: it moves to the centre |
| `Enter` | Open the selected entry |
| Click in the **parent** column | A directory becomes the active directory. A file makes the parent the active directory, with that file selected. |
| Click in the **preview** column | A directory becomes the active directory (two levels down). A file makes the previewed directory the active directory, with that file selected. |

### Cost and freshness

- **Directory reads.** The active and parent listings are read when the active
  directory changes: one extra `read_dir`, for the parent. The preview listing is
  read off the UI thread each time the cursor lands on a new directory. Only the
  newest request publishes, so holding `Down` through a column of large folders
  never blocks a frame.
- **Change watching.** The active and parent directories are watched for changes.
  The preview is re-read when the cursor moves onto it or when sorting or
  hidden-file visibility changes, not on filesystem events.
- **Empty folder.** When the active directory is empty (or the filter hides
  everything), the content area shows the empty-folder placeholder instead of
  columns.

---

## Implementation Map

| Concern | Location |
|---------|----------|
| Mode enum and config parsing | `crates/arca-engine/src/config.rs` (`ViewMode`) |
| Miller column model (parent, active) | `crates/arca-engine/src/state.rs` (`rebuild_miller_columns`) |
| Miller preview loader | `crates/arca-engine/src/miller_preview.rs`, exposed as `AppState::request_miller_preview` / `miller_preview` |
| Rendering of all three modes | `crates/arca-ui/src/content.rs` (`build_grid`, `build_list`, `build_miller`, `miller_slots`) |
| Mode-specific keys | `crates/arca-ui/src/app.rs` (`handle_keys`) |
