# Arca Icon Assets

Arca maintains its own solid, vibrant icon set designed for Wayland desktop file
management, inspired by modern Windows 11 (Fluent Design / Files App) and macOS
visual languages.

Optics has no SVG parser (Optics ADR-0174). Arca decodes each SVG here itself
(`crates/arca-ui/src/icons/vector.rs`, ADR-0025), lowers it to optics-ui vector
commands, and registers the set on the window's `optics_ui` context at start-up
(`IconSet::register` in `crates/arca-ui/src/icons.rs`). Glyphs draw through the
native optics-ui icon widgets — vector-crisp at any scale (from 12px list glyphs
to 46px grid cards and 92px hero icons).

## Authoring rules

The decoder accepts the subset icon artwork needs and rejects the rest, so a
non-conforming asset fails `cargo test` (`every_asset_decodes`) instead of
rendering wrongly:

- Elements: `<g>`, `<path>`, `<rect>` (with `rx`/`ry`), `<circle>`, `<ellipse>`,
  `<line>`, `<polyline>`, `<polygon>`; `transform` on any of them.
- Paint: solid colours or `currentColor` in `fill` / `stroke`, with `opacity`,
  `fill-opacity` and `stroke-opacity`. No gradients, patterns, `style`
  attributes, `<use>`, `<text>`, `<image>`, masks, clips or filters.
- Holes: the icon rasterizer fills with the non-zero rule and ignores
  `fill-rule`, so draw a hole with the opposite winding to its outline.
- Stroke width, caps and joins come from the widget, not the file.

## Design Philosophy

- **Solid & Filled Style**: Solid geometry instead of thin wireframe strokes,
  providing clear visual weight and instant recognizability.
- **Lively Modern Palette**:
  - Folders: warm golden amber (`#FFB900` / `#D97706`) with white document peeking
  - Documents & Text: crisp page with modern vibrant blue lines (`#2563EB` / `#3B82F6`)
  - Images / Photos: vibrant violet card (`#8B5CF6`) with scenic peaks and golden sun
  - Audio / Music: energetic coral/orange badge (`#F97316`) with beamed eighth notes
  - Video / Movies: crimson/rose clapperboard (`#E11D48`) with play glyph
  - Archives: warm amber box (`#D97706` / `#F59E0B`) with central zipper slider
  - Source Code: vibrant emerald green (`#10B981`) with `< / >` tags
  - Terminal & Executables: sleek slate console (`#1E293B`) with colored window controls and cyan `>_` prompt
  - Monitor / Desktop: sleek metallic display (`#334155`) with royal indigo desktop wallpaper
  - Home: warm friendly blue house (`#3B82F6` / `#1D4ED8`) with amber doorway glow
  - Downloads: deep cyan tray (`#0284C7`) with bold download arrow
  - Trash: ruby red wastebasket (`#EF4444` / `#DC2626`)
  - Git Branch: Git coral (`#F05032`) branching nodes
  - Symlink: vibrant teal (`#0D9488`) interlocking chain links
- **Light & Dark Mode Support**:
  - Chrome navigation controls (arrows, chevrons, close, plus, eye, search, view modes)
    use `fill="currentColor"` so optics-ui dynamically tints them according to
    the live theme (crisp dark charcoal `#1D212C` in light mode, bright off-white
    `#F4F6FC` in dark mode, active/hover colors when interacted with).
  - Colorful content and file type icons are tuned with high-contrast luminance and
    saturated tones that look radiant against both dark backgrounds (`rgb(25, 28, 40)`)
    and light surfaces (`rgb(243, 245, 249)`).
