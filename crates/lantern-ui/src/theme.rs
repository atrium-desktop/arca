//! Lantern's visual system. Colours and geometry mirror the aegis-design
//! application tokens (`aegis-dev/crates/aegis-design`) — Lantern ships as a
//! companion application for the aegis desktop, so both colour schemes read
//! as the same product while remaining contrast-safe.

use iris::{Align, Color, Frame, LayoutOpts, Theme};

#[derive(Clone, Copy)]
pub struct Tones {
    pub window: Color,
    pub tab_bar: Color,
    pub toolbar: Color,
    pub sidebar: Color,
    pub content: Color,
    pub card: Color,
    pub elevated: Color,
    pub status_bar: Color,
    pub selected: Color,
    pub muted: Color,
    pub preview_well: Color,
    pub popover: Color,
    pub popover_border: Color,
    pub scrim: Color,
}

impl Tones {
    pub fn from_theme(theme: &Theme) -> Tones {
        if theme.is_dark() {
            Tones {
                // Stepped surface family around the aegis application
                // surface rgb(25, 28, 40).
                window: Color::rgba(25, 28, 40, 255),
                tab_bar: Color::rgba(28, 32, 46, 255),
                toolbar: Color::rgba(32, 36, 51, 255),
                sidebar: Color::rgba(28, 32, 46, 255),
                content: Color::rgba(25, 28, 40, 255),
                card: Color::rgba(35, 40, 56, 255),
                elevated: Color::rgba(41, 47, 65, 255),
                status_bar: Color::rgba(28, 32, 46, 255),
                // Selection is the shared low-alpha accent wash, not a rail
                // or an outline.
                selected: Color::rgba(102, 156, 255, 56),
                muted: Color::rgba(160, 168, 188, 255),
                preview_well: Color::rgba(21, 23, 34, 255),
                // aegis popover/scrim materials for floating panels.
                popover: Color::rgba(255, 255, 255, 110),
                popover_border: Color::rgba(255, 255, 255, 72),
                scrim: Color::rgba(8, 10, 18, 118),
            }
        } else {
            Tones {
                window: Color::rgba(243, 245, 249, 255),
                tab_bar: Color::rgba(237, 240, 246, 255),
                toolbar: Color::rgba(249, 251, 255, 255),
                sidebar: Color::rgba(237, 240, 246, 255),
                content: Color::rgba(249, 251, 255, 255),
                card: Color::rgba(255, 255, 255, 255),
                elevated: Color::rgba(255, 255, 255, 255),
                status_bar: Color::rgba(237, 240, 246, 255),
                selected: Color::rgba(43, 101, 232, 44),
                muted: Color::rgba(99, 105, 123, 255),
                preview_well: Color::rgba(237, 240, 246, 255),
                popover: Color::rgba(250, 251, 253, 216),
                popover_border: Color::rgba(28, 32, 44, 30),
                scrim: Color::rgba(28, 32, 44, 104),
            }
        }
    }
}

/// Shared scrollbar geometry (aegis `Strokes::scrollbar`); also read by the
/// list header to mirror the scroll gutter.
pub(crate) const SCROLLBAR_W: f32 = 5.0;

pub fn branded_theme(dark: bool) -> Theme {
    let base = if dark { Theme::dark() } else { Theme::light() };
    let colored = if dark {
        base.with_bg(Color::rgba(25, 28, 40, 255))
            .with_fg(Color::rgba(244, 246, 252, 255))
            .with_accent(Color::rgba(102, 156, 255, 255))
            .with_border(Color::rgba(255, 255, 255, 42))
            .with_hover(Color::rgba(255, 255, 255, 24))
            .with_active(Color::rgba(102, 156, 255, 56))
    } else {
        base.with_bg(Color::rgba(243, 245, 249, 255))
            .with_fg(Color::rgba(29, 33, 44, 255))
            .with_accent(Color::rgba(43, 101, 232, 255))
            .with_border(Color::rgba(28, 32, 44, 32))
            .with_hover(Color::rgba(28, 32, 44, 12))
            .with_active(Color::rgba(43, 101, 232, 44))
    };
    // Geometry follows the aegis control/hairline/scrollbar tokens. No
    // active_indicator_width: the accent rail is off-language for aegis
    // applications, and it defaults to 0 in lens.
    colored
        .with_corner_radius(12.0)
        .with_border_width(1.0)
        .with_scrollbar_width(SCROLLBAR_W)
        .with_scrollbar_radius(2.5)
        .with_font_size(14.0)
}

pub fn label_colored(frame: &mut Frame, text: &str, color: Color) {
    let theme = frame.theme();
    frame.set_theme(theme.with_fg(color));
    frame.label(text);
    frame.set_theme(theme);
}

/// Muted/coloured text for fixed-height chrome (status bar, section
/// headers, list metadata). The compact variant draws vertically centred in
/// its slot — the regular padded label would overflow small boxes downward.
pub fn label_colored_sized(frame: &mut Frame, text: &str, size: f32, color: Color) {
    let theme = frame.theme();
    frame.set_theme(theme.with_fg(color));
    frame.label_compact_sized(text, size);
    frame.set_theme(theme);
}

/// Wrapped label with every line centred and the block clamped to
/// `max_lines`, ellipsizing the last visible line. Tiled cards read ragged
/// with left-aligned text, and unbounded wrapping overflowed the fixed card
/// height on long names.
pub(crate) fn label_centered(
    frame: &mut Frame,
    text: &str,
    size: f32,
    max_width: f32,
    max_lines: usize,
) {
    let lines = break_lines(frame, text, size, max_width, max_lines);
    frame.column_ex(
        &LayoutOpts {
            gap: 2.0,
            cross: Align::Center,
            ..Default::default()
        },
        |frame| {
            for line in lines {
                frame.label_compact_sized(&line, size);
            }
        },
    );
}

/// Split `text` into at most `max_lines` lines that each fit `max_width`.
/// Breaks prefer whitespace, fall back to CJK-style per-character breaks,
/// and a still-overflowing final line is ellipsized.
pub(crate) fn break_lines(
    frame: &Frame,
    text: &str,
    size: f32,
    max_width: f32,
    max_lines: usize,
) -> Vec<String> {
    if frame.measure_text(text, size).width <= max_width {
        return vec![text.to_string()];
    }
    let mut lines: Vec<String> = Vec::new();
    let mut rest = text.trim_start();
    while !rest.is_empty() && lines.len() < max_lines {
        let fit = fitting_prefix(frame, rest, size, max_width);
        if fit >= rest.len() {
            lines.push(rest.to_string());
            rest = "";
            break;
        }
        // Prefer a word boundary; an unbroken token (CJK names, URLs) is
        // hard-cut at the fitting character.
        let (line, next) = match rest[..fit].rfind(char::is_whitespace).filter(|&sp| sp > 0) {
            Some(sp) => (rest[..sp].trim_end(), rest[sp..].trim_start()),
            None => (rest[..fit].trim_end(), rest[fit..].trim_start()),
        };
        if line.is_empty() {
            // A space-rich prefix fit nothing usable; hard-cut instead.
            lines.push(rest[..fit].to_string());
            rest = rest[fit..].trim_start();
        } else {
            lines.push(line.to_string());
            rest = next;
        }
    }
    if !rest.is_empty() {
        let last = lines.last_mut().expect("max_lines >= 1");
        while !last.is_empty() && frame.measure_text(&format!("{last}…"), size).width > max_width {
            last.pop();
        }
        let trimmed = last.trim_end().len();
        last.truncate(trimmed);
        last.push('…');
    }
    lines
}

/// Longest char-boundary byte prefix of `text` measuring at most
/// `max_width` (binary search; widths are monotonic enough for layout).
fn fitting_prefix(frame: &Frame, text: &str, size: f32, max_width: f32) -> usize {
    let bounds: Vec<usize> = text
        .char_indices()
        .map(|(i, _)| i)
        .chain(std::iter::once(text.len()))
        .collect();
    let (mut lo, mut hi) = (0usize, bounds.len() - 1); // lo fits, hi unknown
    while lo < hi {
        let mid = (lo + hi).div_ceil(2);
        if frame.measure_text(&text[..bounds[mid]], size).width <= max_width {
            lo = mid;
        } else {
            hi = mid - 1;
        }
    }
    bounds[lo]
}
