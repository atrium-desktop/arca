# Arca icon assets

Arca maintains its own icon set. Each SVG here is registered with lens
at runtime (`lens_icon_register_svg`, via the lazy table in
`crates/arca-ui/src/icons.rs`) and drawn through the native lens icon
widgets — vector-crisp at any size and tinted by the live theme, exactly
like lens's built-in set. No rasterization or generation step is involved:
these SVGs are the single source of truth.

Sources (both permissively licensed, glyphs unmodified):

- most icons — Feather Icons (MIT, https://feathericons.com)
- `star-rounded*.svg` — Material Icons (Apache 2.0), see
  `LICENSE-material-rounded.txt`

To add or change an icon: drop the SVG in this directory and add the
matching `AssetId` variant (PascalCase of the file name) plus its
`include_str!` arm in `crates/arca-ui/src/icons.rs`. Keep glyphs to the
24×24, stroke-or-fill, single-colour style of the existing set — lens draws
runtime icons with the same conventions (2/24 stroke weight, theme colour,
nonzero fill).
