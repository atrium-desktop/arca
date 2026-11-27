//! SVG → optics-ui vector icon lowering.
//!
//! Optics v0.0.88 carries no format parsers (Optics ADR-0174): an icon is an
//! `optics_ui_icon_desc`, a command stream in a 24×24 logical box split into
//! colour runs. This module decodes Arca's SVG artwork with `roxmltree` and
//! `svgtypes` and lowers it into that form (ADR-0025).
//!
//! The supported subset is what icon artwork needs: `<g>`, `<path>`, `<rect>`
//! (with `rx`/`ry`), `<circle>`, `<ellipse>`, `<line>`, `<polyline>` and
//! `<polygon>`; `transform`; solid or `currentColor` `fill` / `stroke`;
//! `opacity` and `fill-opacity` / `stroke-opacity`. Anything that would change
//! the rendered result but cannot be represented (gradients, patterns,
//! `<use>`, `<image>`, `<text>`, masks, clips, filters) is rejected with a
//! [`DecodeError`] rather than silently dropped, so a new asset that strays
//! outside the subset fails the `every_asset_decodes` test instead of
//! shipping a wrong glyph.
//!
//! Each painted element becomes its own colour run, so overlapping shapes
//! composite exactly as SVG paints them. Paint semantics follow the icon
//! runtime: `currentColor` becomes run colour
//! `0`, which the widget tints with the live theme foreground; a solid colour
//! becomes a premultiplied `0xAARRGGBB` with every applicable opacity folded
//! into alpha. Stroke width, caps and joins are chosen by the widget, not the
//! artwork. The icon rasterizer fills with the non-zero rule, so `fill-rule`
//! is not carried; holes must be drawn with opposite winding (all current
//! assets are).

use std::fmt;
use std::str::FromStr;

use optics_ui_sys::{
    optics_gfx_color, optics_ui_icon_cmd, optics_ui_icon_cmd_kind as Kind, optics_ui_icon_paint,
    optics_ui_icon_run,
};

/// The icon runtime's logical box edge, in icon units.
const ICON_BOX: f64 = 24.0;

/// Cubic Bézier circle-quadrant control distance (4/3·(√2−1)).
const KAPPA: f64 = 0.552_284_749_830_793_4;

/// A decoded icon: the command stream and the colour runs that cover it.
#[derive(Debug, Clone, Default)]
pub(crate) struct VectorIcon {
    pub(crate) cmds: Vec<optics_ui_icon_cmd>,
    pub(crate) runs: Vec<optics_ui_icon_run>,
}

/// Why an SVG could not be lowered to a vector icon.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum DecodeError {
    Xml(String),
    NotSvg,
    Attribute {
        element: String,
        name: &'static str,
        value: String,
    },
    Unsupported {
        element: String,
        what: String,
    },
    Empty,
}

impl fmt::Display for DecodeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DecodeError::Xml(e) => write!(f, "malformed XML: {e}"),
            DecodeError::NotSvg => write!(f, "root element is not <svg>"),
            DecodeError::Attribute {
                element,
                name,
                value,
            } => {
                write!(f, "<{element}> has an invalid {name}=\"{value}\"")
            }
            DecodeError::Unsupported { element, what } => {
                write!(f, "<{element}> uses unsupported {what}")
            }
            DecodeError::Empty => write!(f, "no painted geometry"),
        }
    }
}

impl std::error::Error for DecodeError {}

/// Decode `svg` and lower it into the 24×24 icon box. A non-square viewBox is
/// scaled uniformly and centred, matching `preserveAspectRatio="xMidYMid"`.
pub(crate) fn decode(svg: &str) -> Result<VectorIcon, DecodeError> {
    let doc = roxmltree::Document::parse(svg).map_err(|e| DecodeError::Xml(e.to_string()))?;
    let root = doc.root_element();
    if root.tag_name().name() != "svg" {
        return Err(DecodeError::NotSvg);
    }

    let view_box = match root.attribute("viewBox") {
        Some(v) => svgtypes::ViewBox::from_str(v).map_err(|_| attr_err(root, "viewBox", v))?,
        None => svgtypes::ViewBox::new(
            0.0,
            0.0,
            length(root, "width")?.unwrap_or(ICON_BOX),
            length(root, "height")?.unwrap_or(ICON_BOX),
        ),
    };
    let max_dim = view_box.w.max(view_box.h);
    if max_dim <= 0.0 {
        return Err(attr_err(
            root,
            "viewBox",
            root.attribute("viewBox").unwrap_or(""),
        ));
    }
    let scale = ICON_BOX / max_dim;
    let to_box = Affine([
        scale,
        0.0,
        0.0,
        scale,
        -view_box.x * scale + (ICON_BOX - view_box.w * scale) * 0.5,
        -view_box.y * scale + (ICON_BOX - view_box.h * scale) * 0.5,
    ]);

    let mut out = Lowering::default();
    let style = Style::initial().inherit(root)?;
    for child in root.children().filter(roxmltree::Node::is_element) {
        out.node(child, &style, to_box)?;
    }
    if out.icon.cmds.is_empty() {
        return Err(DecodeError::Empty);
    }
    Ok(out.icon)
}

// ── Painting state ──────────────────────────────────────────────────────────

/// A resolved paint, before opacity.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Paint {
    None,
    CurrentColor,
    Solid(svgtypes::Color),
}

/// The inherited presentation attributes that affect icon output.
#[derive(Debug, Clone, Copy)]
struct Style {
    fill: Paint,
    fill_opacity: f64,
    stroke: Paint,
    stroke_opacity: f64,
    /// Product of every ancestor's `opacity` (not inherited per spec, but
    /// group opacity multiplies down; icons have no overlapping translucent
    /// group contents for which the difference would show).
    opacity: f64,
}

impl Style {
    /// SVG initial values: black fill, no stroke, fully opaque.
    fn initial() -> Self {
        Style {
            fill: Paint::Solid(svgtypes::Color::black()),
            fill_opacity: 1.0,
            stroke: Paint::None,
            stroke_opacity: 1.0,
            opacity: 1.0,
        }
    }

    /// Apply `node`'s own presentation attributes on top of the inherited ones.
    fn inherit(mut self, node: roxmltree::Node<'_, '_>) -> Result<Self, DecodeError> {
        if node.has_attribute("style") {
            return Err(unsupported(node, "the style attribute"));
        }
        if let Some(v) = node.attribute("fill") {
            self.fill = paint(node, "fill", v, self.fill)?;
        }
        if let Some(v) = node.attribute("stroke") {
            self.stroke = paint(node, "stroke", v, self.stroke)?;
        }
        if let Some(o) = number(node, "fill-opacity")? {
            self.fill_opacity = o.clamp(0.0, 1.0);
        }
        if let Some(o) = number(node, "stroke-opacity")? {
            self.stroke_opacity = o.clamp(0.0, 1.0);
        }
        if let Some(o) = number(node, "opacity")? {
            self.opacity *= o.clamp(0.0, 1.0);
        }
        for name in ["clip-path", "mask", "filter"] {
            if node.attribute(name).is_some_and(|v| v != "none") {
                return Err(unsupported(node, name));
            }
        }
        Ok(self)
    }
}

/// Resolve a `fill` / `stroke` value. `inherited` answers `inherit`.
fn paint(
    node: roxmltree::Node<'_, '_>,
    name: &'static str,
    value: &str,
    inherited: Paint,
) -> Result<Paint, DecodeError> {
    match svgtypes::Paint::from_str(value).map_err(|_| attr_err(node, name, value))? {
        svgtypes::Paint::None => Ok(Paint::None),
        svgtypes::Paint::Inherit => Ok(inherited),
        svgtypes::Paint::CurrentColor => Ok(Paint::CurrentColor),
        svgtypes::Paint::Color(c) => Ok(Paint::Solid(c)),
        _ => Err(unsupported(
            node,
            &format!("{name}=\"{value}\" (only solid colours)"),
        )),
    }
}

/// The run colour for `paint` at `opacity`, or `None` when nothing would be
/// visible. `0` is reserved by the runtime for "theme foreground", so a solid
/// colour is never allowed to collapse to it.
fn run_color(paint: Paint, opacity: f64) -> Option<optics_gfx_color> {
    match paint {
        Paint::None => None,
        Paint::CurrentColor => Some(0),
        Paint::Solid(c) => {
            let a = (f64::from(c.alpha) * opacity).round().clamp(0.0, 255.0);
            if a == 0.0 {
                return None;
            }
            let premul = |ch: u8| (f64::from(ch) * a / 255.0).round() as u32;
            Some(
                ((a as u32) << 24)
                    | (premul(c.red) << 16)
                    | (premul(c.green) << 8)
                    | premul(c.blue),
            )
        }
    }
}

// ── Geometry ────────────────────────────────────────────────────────────────

/// A 2D affine transform `[a b c d e f]` in SVG matrix order.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Affine([f64; 6]);

impl Affine {
    /// `self ∘ other`: apply `other` first, then `self`.
    fn then_inner(self, other: Affine) -> Affine {
        let [a, b, c, d, e, f] = self.0;
        let [a2, b2, c2, d2, e2, f2] = other.0;
        Affine([
            a * a2 + c * b2,
            b * a2 + d * b2,
            a * c2 + c * d2,
            b * c2 + d * d2,
            a * e2 + c * f2 + e,
            b * e2 + d * f2 + f,
        ])
    }

    fn apply(self, x: f64, y: f64) -> (f32, f32) {
        let [a, b, c, d, e, f] = self.0;
        ((a * x + c * y + e) as f32, (b * x + d * y + f) as f32)
    }

    /// Uniform scale + rotation (+ translation): circles stay circles.
    fn is_similarity(self) -> bool {
        let [a, b, c, d, ..] = self.0;
        (a - d).abs() < 1e-9 && (b + c).abs() < 1e-9
    }

    /// Positive axis-aligned scale (+ translation): rects stay rects.
    fn is_axis_aligned(self) -> bool {
        let [a, b, c, d, ..] = self.0;
        b.abs() < 1e-9 && c.abs() < 1e-9 && a > 0.0 && d > 0.0
    }

    fn uniform_scale(self) -> f64 {
        let [a, b, ..] = self.0;
        a.hypot(b)
    }
}

/// Accumulates commands and runs for one icon.
#[derive(Default)]
struct Lowering {
    icon: VectorIcon,
}

impl Lowering {
    fn node(
        &mut self,
        node: roxmltree::Node<'_, '_>,
        parent: &Style,
        parent_ts: Affine,
    ) -> Result<(), DecodeError> {
        let name = node.tag_name().name();
        match name {
            // Non-rendering metadata.
            "title" | "desc" | "metadata" => return Ok(()),
            "g" | "path" | "rect" | "circle" | "ellipse" | "line" | "polyline" | "polygon" => {}
            _ => return Err(unsupported(node, "element")),
        }
        if node.attribute("display") == Some("none") {
            return Ok(());
        }

        let style = parent.inherit(node)?;
        let ts = match node.attribute("transform") {
            Some(v) => {
                let t =
                    svgtypes::Transform::from_str(v).map_err(|_| attr_err(node, "transform", v))?;
                parent_ts.then_inner(Affine([t.a, t.b, t.c, t.d, t.e, t.f]))
            }
            None => parent_ts,
        };

        if name == "g" {
            for child in node.children().filter(roxmltree::Node::is_element) {
                self.node(child, &style, ts)?;
            }
            return Ok(());
        }

        let mut geom = Vec::new();
        shape(node, ts, &mut geom)?;
        if geom.is_empty() {
            return Ok(());
        }
        // A shape with both fill and stroke paints the fill first, then the
        // stroke on top, as SVG's default paint-order does.
        if let Some(color) = run_color(style.fill, style.fill_opacity * style.opacity) {
            self.push_run(
                &geom,
                color,
                optics_ui_icon_paint::OPTICS_UI_ICON_PAINT_FILL,
            );
        }
        if let Some(color) = run_color(style.stroke, style.stroke_opacity * style.opacity) {
            self.push_run(
                &geom,
                color,
                optics_ui_icon_paint::OPTICS_UI_ICON_PAINT_STROKE,
            );
        }
        Ok(())
    }

    /// Append `geom` as its own run.
    ///
    /// Runs are never merged, even when adjacent shapes share a colour: SVG
    /// fills every element independently, but the runtime fills a run as one
    /// non-zero path, so merging a counter-clockwise shape into a clockwise
    /// one of the same colour would cancel their overlap into a hole.
    fn push_run(
        &mut self,
        geom: &[optics_ui_icon_cmd],
        color: optics_gfx_color,
        paint: optics_ui_icon_paint,
    ) {
        let first_cmd = self.icon.cmds.len() as u32;
        let count = geom.len() as u32;
        self.icon.cmds.extend_from_slice(geom);
        self.icon.runs.push(optics_ui_icon_run {
            first_cmd,
            count,
            color,
            paint,
            _pad: [0; 3],
        });
    }
}

fn cmd(kind: Kind, params: &[f32]) -> optics_ui_icon_cmd {
    let mut p = [0.0f32; 6];
    p[..params.len()].copy_from_slice(params);
    optics_ui_icon_cmd {
        kind,
        _pad: [0; 3],
        params: p,
    }
}

/// Emit `node`'s outline in icon-box coordinates.
fn shape(
    node: roxmltree::Node<'_, '_>,
    ts: Affine,
    out: &mut Vec<optics_ui_icon_cmd>,
) -> Result<(), DecodeError> {
    let num = |name| Ok::<f64, DecodeError>(number(node, name)?.unwrap_or(0.0));
    match node.tag_name().name() {
        "path" => {
            let Some(d) = node.attribute("d") else {
                return Ok(());
            };
            for seg in svgtypes::SimplifyingPathParser::from(d) {
                use svgtypes::SimplePathSegment as S;
                match seg.map_err(|_| attr_err(node, "d", d))? {
                    S::MoveTo { x, y } => {
                        let (x, y) = ts.apply(x, y);
                        out.push(cmd(Kind::OPTICS_UI_ICON_CMD_MOVE, &[x, y]));
                    }
                    S::LineTo { x, y } => {
                        let (x, y) = ts.apply(x, y);
                        out.push(cmd(Kind::OPTICS_UI_ICON_CMD_LINE, &[x, y]));
                    }
                    S::CurveTo {
                        x1,
                        y1,
                        x2,
                        y2,
                        x,
                        y,
                    } => {
                        let (x1, y1) = ts.apply(x1, y1);
                        let (x2, y2) = ts.apply(x2, y2);
                        let (x, y) = ts.apply(x, y);
                        out.push(cmd(Kind::OPTICS_UI_ICON_CMD_CUBIC, &[x1, y1, x2, y2, x, y]));
                    }
                    S::Quadratic { x1, y1, x, y } => {
                        let (x1, y1) = ts.apply(x1, y1);
                        let (x, y) = ts.apply(x, y);
                        out.push(cmd(Kind::OPTICS_UI_ICON_CMD_QUAD, &[x1, y1, x, y]));
                    }
                    S::ClosePath => out.push(cmd(Kind::OPTICS_UI_ICON_CMD_CLOSE, &[])),
                }
            }
        }
        "rect" => {
            let (x, y, w, h) = (num("x")?, num("y")?, num("width")?, num("height")?);
            if w <= 0.0 || h <= 0.0 {
                return Ok(());
            }
            // An absent rx/ry takes the other's value (SVG 1.1 §9.2).
            let (rx, ry) = match (number(node, "rx")?, number(node, "ry")?) {
                (None, None) => (0.0, 0.0),
                (Some(r), None) | (None, Some(r)) => (r, r),
                (Some(rx), Some(ry)) => (rx, ry),
            };
            let (rx, ry) = (rx.clamp(0.0, w / 2.0), ry.clamp(0.0, h / 2.0));
            if rx == 0.0 && ry == 0.0 && ts.is_axis_aligned() {
                let (x0, y0) = ts.apply(x, y);
                let (x1, y1) = ts.apply(x + w, y + h);
                out.push(cmd(
                    Kind::OPTICS_UI_ICON_CMD_RECT,
                    &[x0, y0, x1 - x0, y1 - y0],
                ));
            } else {
                rounded_rect(ts, x, y, w, h, rx, ry, out);
            }
        }
        "circle" => {
            let (cx, cy, r) = (num("cx")?, num("cy")?, num("r")?);
            if r <= 0.0 {
                return Ok(());
            }
            if ts.is_similarity() {
                let (x, y) = ts.apply(cx, cy);
                let r = (r * ts.uniform_scale()) as f32;
                out.push(cmd(Kind::OPTICS_UI_ICON_CMD_CIRCLE, &[x, y, r]));
            } else {
                ellipse(ts, cx, cy, r, r, out);
            }
        }
        "ellipse" => {
            let (cx, cy, rx, ry) = (num("cx")?, num("cy")?, num("rx")?, num("ry")?);
            if rx > 0.0 && ry > 0.0 {
                ellipse(ts, cx, cy, rx, ry, out);
            }
        }
        "line" => {
            let (x1, y1) = ts.apply(num("x1")?, num("y1")?);
            let (x2, y2) = ts.apply(num("x2")?, num("y2")?);
            out.push(cmd(Kind::OPTICS_UI_ICON_CMD_MOVE, &[x1, y1]));
            out.push(cmd(Kind::OPTICS_UI_ICON_CMD_LINE, &[x2, y2]));
        }
        name @ ("polyline" | "polygon") => {
            let Some(points) = node.attribute("points") else {
                return Ok(());
            };
            let mut first = true;
            for (px, py) in svgtypes::PointsParser::from(points) {
                let (x, y) = ts.apply(px, py);
                let kind = if first {
                    Kind::OPTICS_UI_ICON_CMD_MOVE
                } else {
                    Kind::OPTICS_UI_ICON_CMD_LINE
                };
                out.push(cmd(kind, &[x, y]));
                first = false;
            }
            if name == "polygon" && !first {
                out.push(cmd(Kind::OPTICS_UI_ICON_CMD_CLOSE, &[]));
            }
        }
        _ => unreachable!("filtered by Lowering::node"),
    }
    Ok(())
}

/// A (possibly rounded) rectangle as a closed path, for transforms that do
/// not keep it axis-aligned or for non-zero corner radii.
#[expect(
    clippy::too_many_arguments,
    reason = "SVG rect geometry: transform, origin, size and two corner radii"
)]
fn rounded_rect(
    ts: Affine,
    x: f64,
    y: f64,
    w: f64,
    h: f64,
    rx: f64,
    ry: f64,
    out: &mut Vec<optics_ui_icon_cmd>,
) {
    let (kx, ky) = (rx * KAPPA, ry * KAPPA);
    let (r, b) = (x + w, y + h);
    let mv = |out: &mut Vec<_>, px, py| {
        let (px, py) = ts.apply(px, py);
        out.push(cmd(Kind::OPTICS_UI_ICON_CMD_MOVE, &[px, py]));
    };
    let ln = |out: &mut Vec<_>, px, py| {
        let (px, py) = ts.apply(px, py);
        out.push(cmd(Kind::OPTICS_UI_ICON_CMD_LINE, &[px, py]));
    };
    let cu = |out: &mut Vec<_>, c1: (f64, f64), c2: (f64, f64), p: (f64, f64)| {
        let (a, b) = ts.apply(c1.0, c1.1);
        let (c, d) = ts.apply(c2.0, c2.1);
        let (e, f) = ts.apply(p.0, p.1);
        out.push(cmd(Kind::OPTICS_UI_ICON_CMD_CUBIC, &[a, b, c, d, e, f]));
    };
    let rounded = rx > 0.0 && ry > 0.0;
    mv(out, x + rx, y);
    ln(out, r - rx, y);
    if rounded {
        cu(out, (r - rx + kx, y), (r, y + ry - ky), (r, y + ry));
    }
    ln(out, r, b - ry);
    if rounded {
        cu(out, (r, b - ry + ky), (r - rx + kx, b), (r - rx, b));
    }
    ln(out, x + rx, b);
    if rounded {
        cu(out, (x + rx - kx, b), (x, b - ry + ky), (x, b - ry));
    }
    ln(out, x, y + ry);
    if rounded {
        cu(out, (x, y + ry - ky), (x + rx - kx, y), (x + rx, y));
    }
    out.push(cmd(Kind::OPTICS_UI_ICON_CMD_CLOSE, &[]));
}

/// An ellipse as four cubic quadrants (clockwise from the rightmost point).
fn ellipse(ts: Affine, cx: f64, cy: f64, rx: f64, ry: f64, out: &mut Vec<optics_ui_icon_cmd>) {
    let (kx, ky) = (rx * KAPPA, ry * KAPPA);
    let (x0, y0) = ts.apply(cx + rx, cy);
    out.push(cmd(Kind::OPTICS_UI_ICON_CMD_MOVE, &[x0, y0]));
    let quadrants = [
        ((cx + rx, cy + ky), (cx + kx, cy + ry), (cx, cy + ry)),
        ((cx - kx, cy + ry), (cx - rx, cy + ky), (cx - rx, cy)),
        ((cx - rx, cy - ky), (cx - kx, cy - ry), (cx, cy - ry)),
        ((cx + kx, cy - ry), (cx + rx, cy - ky), (cx + rx, cy)),
    ];
    for (c1, c2, p) in quadrants {
        let (a, b) = ts.apply(c1.0, c1.1);
        let (c, d) = ts.apply(c2.0, c2.1);
        let (e, f) = ts.apply(p.0, p.1);
        out.push(cmd(Kind::OPTICS_UI_ICON_CMD_CUBIC, &[a, b, c, d, e, f]));
    }
    out.push(cmd(Kind::OPTICS_UI_ICON_CMD_CLOSE, &[]));
}

// ── Attribute helpers ───────────────────────────────────────────────────────

fn number(node: roxmltree::Node<'_, '_>, name: &'static str) -> Result<Option<f64>, DecodeError> {
    node.attribute(name)
        .map(|v| {
            svgtypes::Number::from_str(v)
                .map(|n| n.0)
                .map_err(|_| attr_err(node, name, v))
        })
        .transpose()
}

/// A plain or `px` length (root `width` / `height`); other units are rejected.
fn length(node: roxmltree::Node<'_, '_>, name: &'static str) -> Result<Option<f64>, DecodeError> {
    node.attribute(name)
        .map(|v| {
            let l = svgtypes::Length::from_str(v).map_err(|_| attr_err(node, name, v))?;
            match l.unit {
                svgtypes::LengthUnit::None | svgtypes::LengthUnit::Px => Ok(l.number),
                _ => Err(attr_err(node, name, v)),
            }
        })
        .transpose()
}

fn attr_err(node: roxmltree::Node<'_, '_>, name: &'static str, value: &str) -> DecodeError {
    DecodeError::Attribute {
        element: node.tag_name().name().to_owned(),
        name,
        value: value.to_owned(),
    }
}

fn unsupported(node: roxmltree::Node<'_, '_>, what: &str) -> DecodeError {
    DecodeError::Unsupported {
        element: node.tag_name().name().to_owned(),
        what: what.to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kinds(icon: &VectorIcon) -> Vec<Kind> {
        icon.cmds.iter().map(|c| c.kind).collect()
    }

    #[test]
    fn current_color_fill_is_theme_tinted() {
        let icon = decode(
            r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24">
                 <path fill="currentColor" d="M2 2H22V22Z"/>
               </svg>"#,
        )
        .unwrap();
        assert_eq!(icon.runs.len(), 1);
        assert_eq!(icon.runs[0].color, 0);
        assert_eq!(icon.runs[0].count as usize, icon.cmds.len());
        assert_eq!(
            icon.runs[0].paint,
            optics_ui_icon_paint::OPTICS_UI_ICON_PAINT_FILL
        );
    }

    #[test]
    fn same_colour_shapes_keep_independent_runs() {
        // Clockwise square under a counter-clockwise one: one merged non-zero
        // fill would cancel the overlap, unlike SVG's per-element fill.
        let icon = decode(
            r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24">
                 <path fill="currentColor" d="M2 2H12V12H2Z"/>
                 <path fill="currentColor" d="M6 6V16H16V6Z"/>
               </svg>"#,
        )
        .unwrap();
        assert_eq!(icon.runs.len(), 2);
        assert_eq!((icon.runs[0].first_cmd, icon.runs[0].count), (0, 5));
        assert_eq!((icon.runs[1].first_cmd, icon.runs[1].count), (5, 5));
    }

    #[test]
    fn solid_colours_are_premultiplied_with_opacity() {
        let icon = decode(
            r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24">
                  <rect fill="#FF8000" x="0" y="0" width="4" height="4"/>
                  <rect fill="#FFFFFF" fill-opacity="0.25" x="4" y="4" width="4" height="4"/>
                </svg>"##,
        )
        .unwrap();
        assert_eq!(icon.runs.len(), 2);
        assert_eq!(icon.runs[0].color, 0xFFFF_8000);
        assert_eq!(icon.runs[1].color, 0x4040_4040, "25% white, premultiplied");
    }

    #[test]
    fn plain_rect_and_circle_use_native_primitives() {
        let icon = decode(
            r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24">
                 <rect x="1" y="2" width="3" height="4"/>
                 <circle cx="12" cy="12" r="5"/>
               </svg>"#,
        )
        .unwrap();
        assert_eq!(
            kinds(&icon),
            [
                Kind::OPTICS_UI_ICON_CMD_RECT,
                Kind::OPTICS_UI_ICON_CMD_CIRCLE
            ]
        );
        assert_eq!(&icon.cmds[0].params[..4], &[1.0, 2.0, 3.0, 4.0]);
        assert_eq!(&icon.cmds[1].params[..3], &[12.0, 12.0, 5.0]);
        assert_eq!(icon.runs[0].color, 0xFF00_0000, "SVG initial fill is black");
    }

    #[test]
    fn rounded_or_rotated_rects_become_paths() {
        let icon = decode(
            r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24">
                 <rect x="2" y="2" width="20" height="20" rx="4"/>
                 <rect x="2" y="2" width="4" height="4" transform="rotate(45 4 4)"/>
               </svg>"#,
        )
        .unwrap();
        let k = kinds(&icon);
        assert_eq!(
            k.iter()
                .filter(|k| **k == Kind::OPTICS_UI_ICON_CMD_CUBIC)
                .count(),
            4
        );
        assert_eq!(
            k.iter()
                .filter(|k| **k == Kind::OPTICS_UI_ICON_CMD_CLOSE)
                .count(),
            2
        );
        assert!(!k.contains(&Kind::OPTICS_UI_ICON_CMD_RECT));
    }

    #[test]
    fn view_box_is_scaled_into_the_icon_box() {
        let icon = decode(
            r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 48 24">
                 <path d="M0 0L48 24"/>
               </svg>"#,
        )
        .unwrap();
        // Width 48 → 24 (scale 0.5); height 12 centred in 24 → y offset 6.
        assert_eq!(&icon.cmds[0].params[..2], &[0.0, 6.0]);
        assert_eq!(&icon.cmds[1].params[..2], &[24.0, 18.0]);
    }

    #[test]
    fn group_paint_and_transform_inherit() {
        let icon = decode(
            r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24">
                  <g fill="#00FF00" transform="translate(10 0)">
                    <polygon points="0,0 2,0 2,2"/>
                  </g>
                </svg>"##,
        )
        .unwrap();
        assert_eq!(icon.runs[0].color, 0xFF00_FF00);
        assert_eq!(&icon.cmds[0].params[..2], &[10.0, 0.0]);
        assert_eq!(
            icon.cmds.last().unwrap().kind,
            Kind::OPTICS_UI_ICON_CMD_CLOSE
        );
    }

    #[test]
    fn stroke_only_shapes_are_stroke_runs() {
        let icon = decode(
            r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor">
                 <line x1="1" y1="1" x2="23" y2="23"/>
               </svg>"#,
        )
        .unwrap();
        assert_eq!(icon.runs.len(), 1);
        assert_eq!(
            icon.runs[0].paint,
            optics_ui_icon_paint::OPTICS_UI_ICON_PAINT_STROKE
        );
    }

    #[test]
    fn unrepresentable_input_is_rejected() {
        let gradient = r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24">
                            <path fill="url(#g)" d="M0 0L1 1Z"/></svg>"#;
        assert!(matches!(
            decode(gradient),
            Err(DecodeError::Unsupported { .. })
        ));
        let text = r#"<svg xmlns="http://www.w3.org/2000/svg"><text>hi</text></svg>"#;
        assert!(matches!(decode(text), Err(DecodeError::Unsupported { .. })));
        assert_eq!(
            decode("<svg viewBox=\"0 0 24 24\"></svg>").unwrap_err(),
            DecodeError::Empty
        );
        assert_eq!(decode("<html/>").unwrap_err(), DecodeError::NotSvg);
        assert!(matches!(decode("not xml <<<"), Err(DecodeError::Xml(_))));
    }
}
