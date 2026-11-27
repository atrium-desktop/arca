---
id: ADR-0026
title: "Synchronize with Optics v0.0.89: Per-Corner Radii and Single-Representation Frame Options"
status: accepted
date: 2026-11-27
scope: [build/dependencies, ui/widgets]
superseded_by: null
negative_knowledge: false
---

# 0026. Synchronize with Optics v0.0.89: Per-Corner Radii and Single-Representation Frame Options

- Status: Accepted
- Date: 2026-11-27
- Deciders: Arca Architecture Working Group
- Consulted: Optics Graphics/Bindings Maintainers
- Extends: [ADR-0025](0025-synchronize-with-optics-v0.0.88-and-app-owned-icon-decoding.md)
- References: Optics `v0.0.89`, Optics ADR-0175

---

## Context and Problem Statement

Optics `v0.0.89` (Optics ADR-0175) replaces the scalar corner radius with a
per-corner `CornerRadii` value across `gfx`, `ui` and their Rust bindings,
and removes the combinatorial `optics_ui::Frame` helper variants in favour of
one call per widget that takes an options struct. As with `v0.0.88`, no
compatibility aliases are kept.

Arca's call sites hit these removals:

| Removed (v0.0.88)                                   | Replacement (v0.0.89)                                              |
| --------------------------------------------------- | ------------------------------------------------------------------ |
| `LayoutOpts { radius: r, .. }`                      | `LayoutOpts { corner_radii: CornerRadii::uniform(r), .. }`         |
| `Frame::row_ex(&opts, body)` / `column_ex`          | `Frame::row().with_opts(&opts).show(body)` / `column()`            |
| `Frame::label_compact(t)`                           | `Frame::label(t)`                                                  |
| `Frame::label_compact_sized(t, s)`                  | `Frame::label_with(t, &LabelOpts { size: s, .. })`                 |
| `Frame::label_wrapped_sized(t, s, w)`               | `Frame::label_with(t, &LabelOpts { size: s, wrap: true, max_width: w, .. })` |
| `Frame::textedit_ex(l, b, false, false, false)`     | `Frame::textedit(l, b, &TextEditOpts::default())`                  |
| `Frame::textfield_ex(l, b, tab)`                    | `Frame::textedit(l, b, &TextEditOpts { accept_tab: tab, select_all_on_focus: true, .. })` |
| `Frame::textfield_placeholder(l, b, p)`             | `Frame::textedit(l, b, &TextEditOpts { placeholder: Some(p), .. }).changed` |
| `Frame::textfield_set_caret`                        | `Frame::textedit_set_caret`                                        |
| `Frame::icon_raw(id, size)`                         | `Frame::icon(id, size)` (takes `impl Into<optics_ui_icon_id>`)     |
| `Frame::selectable_icon_sized`                      | `Frame::selectable_with_icon`                                      |

Arca used none of the removed `optics-gfx` canvas variants
(`draw_image_sampled*`, `draw_image_rrect`, `Path::add_round_rect`, …), the
`skin_rect` / `skin_border` helpers, or any raw descriptor with a `radius`
field. Thumbnails reach the canvas through `optics_ui` image widgets, not
direct canvas calls.

## Decision

Repin every Optics dependency (`Cargo.toml`, CI `OPTICS_TAG`, the packaging
manifest example) to `v0.0.89` and migrate every call site to the single
v0.0.89 form in the table above. No Arca-side wrapper reintroduces a removed
name.

- **Uniform radii stay uniform.** Every Arca surface had a single radius, so
  each becomes `CornerRadii::uniform(r)` with the default `corner_curvature`
  of `0.0` (circular arc), the same shape as before. Upstream now draws every
  non-zero radius through the unified analytic RRect SDF, and Arca's smallest
  radius is 4 px. Arca has no pixel-comparison test, so equivalence rests on
  Optics' own GPU/CPU parity suites (`test_sdf_parity`,
  `test_canvas_consistency`), not on an Arca screenshot diff. Adopting
  squircle curvature or asymmetric corners is a separate design change.
- **Behaviour is preserved field-for-field.** The old helpers set only the
  fields named in the table. Two defaults differ from the old zero-initialised
  descriptors and were checked against the C implementation:
  - `LabelOpts::default().align` is `Align::Stretch`, not `OPTICS_UI_START`.
    `optics_ui_label` and its skin special-case only `CENTER` and `END`, so
    `Stretch` lays out exactly like `Start`.
  - Every field that reads `Response::submitted` (location bar, save-name
    field, grid and list rename editors) sets `submit_on_enter: true`. The
    v0.0.89 header defines `submitted` in terms of that flag. The v0.0.89 C
    code ignores the flag and reports Return on every single-line field, so
    nothing changes today. Leaving the flag unset would rely on behaviour the
    contract does not promise.
- **`IconSet`'s subtle icon button keeps its raw descriptor.** The safe
  `Frame::icon_button` still takes no widget id and no box size, so
  `icons::subtle_icon_button` keeps building `optics_ui_button_opts` itself.
- **The activation-token fallback stays.** `optics-window` v0.0.89 still has
  no `xdg_activation_v1::activate(token)` binding, so forwarded IPC tokens
  keep falling back to `window_focus()` (ADR-0023).

## Consequences

### Positive

- Arca uses one spelling per widget, matching upstream docs and examples.
  Later option-struct fields (e.g. `TextEditOpts::readonly`) are available
  without a new helper.
- `corner_radii` lets Arca adopt bezel-flush or per-corner shapes later with
  no further API change.

### Negative

- Struct-literal options are more verbose than positional helpers at
  one-line call sites (`label_with(.., &LabelOpts { size, ..Default::default() })`).

### Verification

- `cargo clippy --workspace --all-targets --all-features` is warning-free.
- `cargo nextest run --locked --workspace` (process per test) passes in full.
- The scroll-damage test was red since the Optics `v0.0.88` sync (`12e33c6`;
  green at `1e1fa89` with `v0.0.86`). Optics `v0.0.87` made `damage_walk`
  report a changed node's previously painted rect as well as its current one,
  which closes a stale-pixel gap but doubles the cost of every moved row. One
  wheel frame over a 700 px List view went from 59 rectangles to more than the
  64-rect `OPTICS_UI_DAMAGE_RECTS_CAPACITY`, so Optics answers with its single
  full-viewport cover (`728000` of `728000` px²). That is correct but not
  local. The suite now checks both sides of the budget:
  - `test_scrolling_virtualized_listing_damages_only_the_viewport_then_settles`
    scrolls a 320 px viewport (39 rectangles) and still requires local damage;
    it fails from about 460 px, so a regression that widens damage is caught.
  - `test_scrolling_a_full_height_listing_past_the_damage_budget_reports_one_cover_then_settles`
    requires the 700 px frame to stay within 64 rectangles, to be exactly one
    full-viewport cover if it covers the window, and to settle to zero damage.
  Keeping full-height scroll damage local needs an upstream Optics change
  (reporting a scrolled subtree through its clip rather than per node).
- The `integration` and `arca-ui` unit-test binaries used to segfault, or
  abort with `free(): invalid pointer`, when their tests shared one process.
  The cause was an Optics bug, fixed in the addendum below. On `v0.0.90` both
  binaries pass in one process (`arca-ui` 92 of 92, `integration` 56 of 56).

## Addendum: Optics v0.0.90

Arca pins Optics `v0.0.90`, which adds one fix on top of `v0.0.89` and no API
change. In `v0.0.88`–`v0.0.89`, `optics_ui_icon_register` grew the per-context
icon registry without initialising the new slots. Headless contexts allocate
with libc `malloc`, so a slot could read as occupied, and
`optics_ui__icon_registry_destroy` then freed its garbage `cmds`/`runs`
pointers. Arca registers more than 16 icons, so every headless `Ui` that
called `UiApp::start` triggered the growth. Valgrind reported 1451 errors in a
single chooser test on `v0.0.89` and none on `v0.0.90`. Device-backed windows
were not affected, because `optics_gfx_device_alloc` zeroes.
