---
id: ADR-0025
title: "Synchronize with Optics v0.0.88: App-Owned SVG Decoding and Per-Context Icon Registration"
status: accepted
date: 2026-11-27
scope: [build/dependencies, rendering/icons, ui/lifecycle]
superseded_by: null
negative_knowledge: true
---

# 0025. Synchronize with Optics v0.0.88: App-Owned SVG Decoding and Per-Context Icon Registration

- Status: Accepted
- Date: 2026-11-27
- Deciders: Arca Architecture Working Group
- Consulted: Optics Graphics/Bindings Maintainers
- Extends: [ADR-0017](0017-synchronize-with-optics-v0.0.86-and-root-test-architecture.md), [ADR-0024](0024-local-optics-source-patch-and-readonly-lockfile-guardrail.md)
- Amends: the icon-registration clause of [ADR-0011](0011-synchronize-with-optics-v0.0.79-and-unified-bindings.md) ("every shipped SVG compiles through the shared vector compiler")
- References: Optics `v0.0.87`, `v0.0.88`, Optics ADR-0174

---

## Context and Problem Statement

Optics `v0.0.87` made `optics-scene`'s image codecs opt-in, and `v0.0.88`
(Optics ADR-0174) reduced Optics to six libraries: `gfx`, `text`, `motion`,
`window`, `ui`, `app`. Optics now ships no format parsers and keeps no
compatibility shims. For Arca the changes that matter are:

1. **`liboptics_svg` and `optics_ui::register_svg_icon` are gone.** Arca
   registered its 35 SVG glyphs (`assets/icons/*.svg`) through that call.
2. **The icon registry is per context.** `Ui::register_icon(cmds, runs, paint)`
   and `Ui::unregister_icon(id)` replace the process-global, never-reclaimed
   table. Ids are generational `(gen << 16) | slot` handles valid only on the
   `optics_ui` they were registered on.
3. **`OPTICS_UI_ICON_NONE` is `0`,** the only "no icon" sentinel. The
   `(optics_ui_icon_id)-1` "invalid" value and `OPTICS_UI_ICON_COUNT` are
   gone. Only four chevrons stay built in (`OPTICS_UI_ICON_BUILTIN_COUNT`).

Arca used none of the removed scene, material or bundled-icon APIs.

## Decision Drivers

- Follow ADR-0174 as written: the application decodes formats and hands
  Optics format-agnostic data. No Arca-side copy of the retired C library.
- No global state where Optics has removed it. Icon ids belong to a context.
- A new or edited asset that the pipeline cannot draw faithfully must fail a
  test, not ship a blank or wrong glyph.
- Keep `assets/icons/*.svg` as the authored source of truth.

## Architectural Decisions

### 1. Repin to `v0.0.88`

The workspace pins `optics-app`, `optics-window`, `optics-ui`,
`optics-ui-sys`, `optics-gfx-sys` and `optics-motion` at tag `v0.0.88`. Linked
`dev` worktrees advance their local lockfile with `cargo update -p optics-…`
against `../optics` (ADR-0024).

### 2. Decode SVG in Arca with `roxmltree` + `svgtypes`

`crates/arca-ui/src/icons/vector.rs` parses each asset with `roxmltree` and
`svgtypes` and lowers it to `optics_ui_icon_cmd` / `optics_ui_icon_run`
in the 24×24 icon box. The two crates are small, pure Rust and already the
foundation of the wider Rust SVG ecosystem. `usvg` was rejected because it
resolves `currentColor` away (see Rejected Alternatives).

The supported subset is what the icon set uses: `g`, `path`, `rect`
(`rx`/`ry`), `circle`, `ellipse`, `line`, `polyline`, `polygon`, `transform`,
solid or `currentColor` fill/stroke, and the opacity attributes. Anything that
would render differently from the file (gradients, `style`, `use`, `text`,
masks, clips, filters) returns a `DecodeError`.

Semantics are kept from the retired C converter:

- `currentColor` becomes run colour `0`, which the widget tints with the theme
  foreground. A solid colour becomes premultiplied `0xAARRGGBB` with opacity
  folded into alpha, and is never allowed to collapse to `0`.
- A non-square viewBox is scaled uniformly and centred.
- `fill-rule` is not carried: the rasterizer fills non-zero. Holes use
  opposite winding (all current assets do).

One semantic is deliberately not kept: **each painted element is its own run.**
The C converter merged adjacent same-colour shapes into one run. The runtime
fills a run as a single non-zero path, so a counter-clockwise shape merged into
a clockwise one of the same colour cancels into a hole, whereas SVG fills each
element independently. Every asset stays well under the runtime's 64-run cap
(the largest has 10 shapes).

Plain rects and circles under an axis-aligned or similarity transform use the
native `RECT` / `CIRCLE` commands. Everything else becomes a path with
clockwise cubic quadrants, matching `optics_gfx_path_add_circle`.

### 3. `IconSet`: a per-context id table owned by `UiApp`

`icons::IconSet` is a `Copy` table of one `optics_ui_icon_id` per `AssetId`.
`IconSet::EMPTY` maps every glyph to `OPTICS_UI_ICON_NONE`.

- `UiApp::start(&mut Ui)` registers the set. The window and chooser call it
  from the `optics-app` start hook through `StartHost::ui()`.
- `UiApp::shutdown(&mut Ui)` unregisters it in the stop hook, next to the
  thumbnail release (Optics ADR-0045 ordering), before Optics destroys the context.
- Builders that hold `app` copy `app.icons`. Frame-only helpers
  (`placeholder`, `row_icon`, `miller_header`, the preview header and body)
  take `icons: &IconSet`. The former free functions (`icon`, `icon_button`,
  `icon_toggle_button`, `selectable_icon[_sized]`, `icon_id`) are now
  `IconSet` methods (`icon`, `button`, `toggle_button`, `selectable[_sized]`,
  `id`).
- Generational ids do not name their context (two fresh contexts hand out
  the same first ids), so `IconSet` records its owning context's address.
  `unregister` panics on any other context instead of silently freeing that
  context's same-numbered glyphs.
- Every headless harness mirrors the start hook. The crate-local
  `test_support::frame` registers whenever the set is not live on the frame's
  context (`IconSet::is_registered_on`: owner address plus a registry
  lookup). The root `tests/common/ui.rs` harness and the hand-built
  `virtualization` / `damage_and_quiescence` set-ups call `UiApp::start`.
  Damage, budget and virtualization suites therefore measure frames that draw
  production glyphs.

The process-global `OnceLock` table and the `u32::MAX` sentinel are deleted.

## Consequences

### Positive

- Arca follows Optics' boundary: no format parser in the graphics stack, no
  global icon state, and icons are reclaimed with their context.
- Multiple `optics_ui` contexts in one process (the tests already create
  dozens) each own their glyphs. `icon_sets_are_per_context` checks this.
- Decoding is covered by unit tests, and `every_asset_decodes` /
  `every_asset_registers` gate every shipped asset. A round-trip render of all
  35 decoded icons against their sources was pixel-identical at 96 px.
- Packagers no longer need `pkgconfig(optics-svg)`.

### Negative & Trade-offs

- Two new crates.io dependencies in `arca-ui` (`roxmltree`, `svgtypes`, plus
  `svgtypes`' `kurbo` / `siphasher`).
- Arca now owns an SVG subset. Artwork that needs more (gradients, for
  example) needs decoder work first. The decoder refuses such files rather
  than drawing them wrongly.
- More builder signatures carry `icons`.

## Rejected Alternatives & Negative Knowledge

### 1. `usvg`

`usvg` normalises the whole tree but resolves `currentColor` to the `color`
property, or black when absent (`usvg` 0.47 `parser/style.rs`). Its output
`Paint` has no current-colour variant, so a theme-tinted glyph is
indistinguishable from a black one. Chrome icons depend on that distinction. It also pulls in a
much larger dependency tree (its default features include text shaping and
system fonts) for 35 icons.

### 2. A thread-local or global id table keyed by context pointer

This would keep the old call sites (`icons::icon_id(x)`) unchanged, but it
recreates the process-global registry that ADR-0174 removed, ties lifetime to
pointer identity, and hides which context a glyph belongs to.

### 3. Pre-converting the SVGs to checked-in Rust command tables

This avoids the runtime parse, but makes generated code the artwork's source
of truth and needs a generator that is itself an SVG decoder. Decoding the
embedded SVG at start-up is cheap (35 small files).

### 4. Keeping run merging

Merging saves a few icon masks but changes coverage for overlapping
same-colour shapes with opposite winding (see Decision 2).

## Links

- Optics ADR-0174 — pure graphics & UI core
- [Icon assets README](../../assets/icons/README.md)
- [Optics migration log](../dev/optics-migration.md)
