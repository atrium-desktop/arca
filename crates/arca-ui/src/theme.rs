//! Arca's visual system. Colours and geometry mirror the tessera-design
//! application tokens (`../tessera-dev/crates/tessera-design`) — Arca ships as a
//! companion application for the tessera desktop, so both colour schemes read
//! as the same product while remaining contrast-safe.

use lens::{Align, Color, Frame, LayoutOpts, Theme};

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
    pub input_bg: Color,
    /// File-name text on a content surface at rest.
    pub file_label: Color,
    /// File-name text on a hovered card (must stay legible on `content`).
    pub file_label_hover: Color,
}

impl Tones {
    pub fn from_theme(theme: &Theme) -> Tones {
        let (r, g, b, _) = theme.bg().components();
        let is_dark = (0.299 * r as f32 + 0.587 * g as f32 + 0.114 * b as f32) < 128.0;
        let surface = theme.surface();
        let surface_sunken = theme.surface_sunken();
        let surface_elevated = theme.surface_elevated();
        if is_dark {
            Tones {
                // Stepped surface family around the tessera application
                // surface rgb(25, 28, 40).
                window: Color::rgba(25, 28, 40, 255),
                tab_bar: Color::rgba(28, 32, 46, 255),
                toolbar: Color::rgba(32, 36, 51, 255),
                sidebar: Color::rgba(28, 32, 46, 255),
                content: Color::rgba(25, 28, 40, 255),
                card: if surface.a() > 0 {
                    surface
                } else {
                    Color::rgba(35, 40, 56, 255)
                },
                elevated: if surface_elevated.a() > 0 {
                    surface_elevated
                } else {
                    Color::rgba(41, 47, 65, 255)
                },
                status_bar: Color::rgba(28, 32, 46, 255),
                // Selection is the shared low-alpha accent wash, not a rail
                // or an outline.
                selected: Color::rgba(102, 156, 255, 56),
                muted: Color::rgba(160, 168, 188, 255),
                preview_well: Color::rgba(21, 23, 34, 255),
                // tessera popover/scrim materials for floating panels.
                // Deep translucent dark slate in dark mode with crisp contrast.
                popover: Color::rgba(24, 28, 40, 235),
                popover_border: Color::rgba(255, 255, 255, 42),
                scrim: Color::rgba(8, 10, 18, 118),
                input_bg: if surface_sunken.a() > 0 {
                    surface_sunken
                } else {
                    Color::rgba(21, 24, 35, 255)
                },
                // ~12.6:1 and ~15.5:1 against `content`: both far above the
                // 4.5:1 floor, hover only slightly brighter.
                file_label: Color::rgba(215, 222, 235, 255),
                file_label_hover: Color::rgba(255, 255, 255, 255),
            }
        } else {
            Tones {
                window: Color::rgba(243, 245, 249, 255),
                tab_bar: Color::rgba(237, 240, 246, 255),
                toolbar: Color::rgba(249, 251, 255, 255),
                sidebar: Color::rgba(237, 240, 246, 255),
                content: Color::rgba(249, 251, 255, 255),
                card: if surface.a() > 0 {
                    surface
                } else {
                    Color::rgba(255, 255, 255, 255)
                },
                elevated: if surface_elevated.a() > 0 {
                    surface_elevated
                } else {
                    Color::rgba(255, 255, 255, 255)
                },
                status_bar: Color::rgba(237, 240, 246, 255),
                selected: Color::rgba(43, 101, 232, 44),
                muted: Color::rgba(99, 105, 123, 255),
                preview_well: Color::rgba(237, 240, 246, 255),
                popover: Color::rgba(250, 251, 253, 216),
                popover_border: Color::rgba(28, 32, 44, 30),
                scrim: Color::rgba(28, 32, 44, 104),
                input_bg: if surface_sunken.a() > 0 {
                    surface_sunken
                } else {
                    Color::rgba(233, 236, 244, 255)
                },
                // The grid label used to be hard-coded near-white, which is
                // 1.3:1 on the light content surface (invisible at rest,
                // 1.04:1 on hover — literally the background colour). These
                // are ~13.9:1 and ~17.5:1, and hover now darkens.
                file_label: Color::rgba(38, 43, 56, 255),
                file_label_hover: Color::rgba(12, 16, 26, 255),
            }
        }
    }
}

/// Shared scrollbar geometry (tessera `Strokes::scrollbar`); also read by the
/// list header to mirror the scroll gutter.
pub(crate) const SCROLLBAR_W: f32 = 5.0;

pub fn branded_theme(dark: bool) -> Theme {
    branded_theme_with_accent(dark, None)
}

pub fn branded_theme_with_accent(
    dark: bool,
    accent: Option<arca_engine::chooser::PromptAccent>,
) -> Theme {
    let base = if dark { Theme::dark() } else { Theme::light() };
    let accent_col = if let Some(a) = accent {
        Color::rgba(a.red, a.green, a.blue, 255)
    } else if dark {
        Color::rgba(102, 156, 255, 255)
    } else {
        Color::rgba(43, 101, 232, 255)
    };

    let colored = if dark {
        base.with_bg(Color::rgba(25, 28, 40, 255))
            .with_fg(Color::rgba(244, 246, 252, 255))
            .with_accent(accent_col)
            .with_border(Color::rgba(255, 255, 255, 42))
            .with_hover(Color::rgba(255, 255, 255, 24))
            .with_active(Color::rgba(
                accent_col.r(),
                accent_col.g(),
                accent_col.b(),
                56,
            ))
            .with_surface(Color::rgba(35, 40, 56, 255))
            .with_surface_sunken(Color::rgba(21, 24, 35, 255))
            .with_surface_elevated(Color::rgba(41, 47, 65, 255))
    } else {
        base.with_bg(Color::rgba(243, 245, 249, 255))
            .with_fg(Color::rgba(29, 33, 44, 255))
            .with_accent(accent_col)
            .with_border(Color::rgba(28, 32, 44, 32))
            .with_hover(Color::rgba(28, 32, 44, 12))
            .with_active(Color::rgba(
                accent_col.r(),
                accent_col.g(),
                accent_col.b(),
                44,
            ))
            .with_surface(Color::rgba(255, 255, 255, 255))
            .with_surface_sunken(Color::rgba(233, 236, 244, 255))
            .with_surface_elevated(Color::rgba(255, 255, 255, 255))
    };
    // Geometry follows the tessera control/hairline/scrollbar tokens. No
    // active_indicator_width: the accent rail is off-language for tessera
    // applications, and it defaults to 0 in lens.
    colored
        .with_corner_radius(8.0)
        .with_border_width(1.0)
        .with_scrollbar_width(SCROLLBAR_W)
        .with_scrollbar_radius(2.5)
        .with_font_size(14.0)
        .with_icon_size(18.0)
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

/// Wrapped label with custom text color and optional pill background.
pub(crate) fn label_centered_styled(
    frame: &mut Frame,
    text: &str,
    size: f32,
    max_width: f32,
    max_lines: usize,
    fg: Color,
    bg: Color,
) {
    let lines = break_lines(frame, text, size, max_width, max_lines);
    let theme = frame.theme();
    frame.set_theme(theme.with_fg(fg));
    let has_bg = (bg.raw() & 0xFF000000) != 0;
    frame.column_ex(
        &LayoutOpts {
            gap: 2.0,
            pad: if has_bg { 2.5 } else { 0.0 },
            bg,
            radius: 4.0,
            cross: Align::Center,
            ..Default::default()
        },
        |frame| {
            for line in lines {
                frame.label_compact_sized(&line, size);
            }
        },
    );
    frame.set_theme(theme);
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
        while !last.is_empty() && frame.measure_text(&format!("{last}…"), size).width > max_width
        {
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

#[cfg(test)]
mod contrast_tests {
    //! WCAG 2.1 relative-luminance contrast floors for body text.
    //!
    //! These exist because the grid label was once a hard-coded near-white:
    //! it read at 1.04:1 against the light content surface (i.e. invisible)
    //! and no test noticed, because the only theme test asserted "does not
    //! panic".

    use super::*;

    fn channel(value: u8) -> f32 {
        let v = value as f32 / 255.0;
        if v <= 0.039_28 {
            v / 12.92
        } else {
            ((v + 0.055) / 1.055).powf(2.4)
        }
    }

    fn luminance(color: Color) -> f32 {
        let (r, g, b, _) = color.components();
        0.2126 * channel(r) + 0.7152 * channel(g) + 0.0722 * channel(b)
    }

    fn contrast(foreground: Color, background: Color) -> f32 {
        let (a, b) = (luminance(foreground), luminance(background));
        let (hi, lo) = if a > b { (a, b) } else { (b, a) };
        (hi + 0.05) / (lo + 0.05)
    }

    /// WCAG AA for normal-size text.
    const AA_NORMAL: f32 = 4.5;

    fn schemes() -> [(&'static str, Tones); 2] {
        [
            ("dark", Tones::from_theme(&branded_theme(true))),
            ("light", Tones::from_theme(&branded_theme(false))),
        ]
    }

    #[test]
    fn file_labels_meet_wcag_aa_on_the_content_surface() {
        for (name, tones) in schemes() {
            for (label, color) in [
                ("file_label", tones.file_label),
                ("file_label_hover", tones.file_label_hover),
            ] {
                let ratio = contrast(color, tones.content);
                assert!(
                    ratio >= AA_NORMAL,
                    "{name} {label} on content is {ratio:.2}:1, below the {AA_NORMAL}:1 floor"
                );
            }
        }
    }

    /// Hover must not reduce legibility — the historical failure was a hover
    /// colour *closer* to the background than the resting colour.
    #[test]
    fn hovered_labels_stay_at_least_as_legible_as_resting() {
        for (name, tones) in schemes() {
            let resting = contrast(tones.file_label, tones.content);
            let hovered = contrast(tones.file_label_hover, tones.content);
            assert!(
                hovered >= resting,
                "{name}: hover contrast {hovered:.2}:1 is worse than resting {resting:.2}:1"
            );
        }
    }

    /// Muted status/label text must remain readable too.
    #[test]
    fn muted_text_meets_a_relaxed_floor() {
        // Muted text is used for secondary metadata at ~11.5px; 4.5:1 is the
        // requirement, and these tokens clear it.
        for (name, tones) in schemes() {
            for surface in [tones.content, tones.toolbar, tones.status_bar] {
                let ratio = contrast(tones.muted, surface);
                assert!(
                    ratio >= AA_NORMAL,
                    "{name} muted text is {ratio:.2}:1 on its surface"
                );
            }
        }
    }

    #[cfg(test)]
    mod theme_behaviour_tests {
        //! Layout and metric behaviour that the visual tokens alone cannot express.

        use super::*;
        use crate::test_support::fixture;

        #[test]
        fn centered_label_breaks_and_ellipsizes() {
            let (_dir, mut app) = fixture();
            let mut ui = lens::Ui::headless().expect("headless ui");
            let input = lens::Input::new((1040.0, 700.0), 1.0 / 60.0);
            ui.frame(&input, |f| {
                app.build(f, &input);
                // A short name stays one line.
                let lines = break_lines(f, "a.txt", 12.5, 114.0, 2);
                assert_eq!(lines, ["a.txt"]);
                // A long CJK name clamps to two lines; the last is ellipsized
                // and every line fits the card's text width.
                let long = "龍猛寺寛度 - 武家坡2021 (Official Music Video) extra long suffix.flac";
                let lines = break_lines(f, long, 12.5, 114.0, 2);
                assert_eq!(lines.len(), 2, "clamped to two lines: {lines:?}");
                assert!(lines[1].ends_with('…'), "second line ellipsized: {lines:?}");
                for line in &lines {
                    assert!(
                        f.measure_text(line, 12.5).width <= 114.0,
                        "line fits: {line:?}"
                    );
                }
                // A single unbreakable token hard-cuts without panicking.
                let token = "supercalifragilisticexpialidocious-supercalifragilisticexpialidocious";
                let lines = break_lines(f, token, 12.5, 114.0, 2);
                assert_eq!(lines.len(), 2);
            });
        }

        #[test]
        fn theme_orthogonal_icon_size_and_selectable_geometry() {
            let dark_theme = crate::theme::branded_theme(true);
            let light_theme = crate::theme::branded_theme(false);
            assert_eq!(dark_theme.icon_size(), 18.0);
            assert_eq!(light_theme.icon_size(), 18.0);
            assert_eq!(dark_theme.font_size(), 14.0);
            assert_eq!(light_theme.font_size(), 14.0);

            let mut ui = lens::Ui::headless().expect("headless ui");
            let input = lens::Input::new((800.0, 600.0), 1.0 / 60.0);
            let mut row_height = 0.0;
            let mut clicked = false;
            for _ in 0..2 {
                ui.frame(&input, |f| {
                    f.push_id("test-selectable");
                    clicked = crate::icons::selectable_icon(
                        f,
                        crate::icons::ids::Folder,
                        "Documents",
                        false,
                    );
                    row_height = f.response().rect.h;
                    f.pop_id();
                });
                if let Ok(snap) = ui.snapshot() {
                    let _ = ui.activate(&snap);
                }
            }
            assert!(!clicked);
            assert_eq!(row_height, 30.0);
        }
    }
}
