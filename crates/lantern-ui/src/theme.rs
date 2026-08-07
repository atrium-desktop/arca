//! Lantern's visual system. The palette remains contrast-safe in both colour
//! schemes while giving the application a recognisable cool-indigo accent.

use iris::{Color, Frame, Theme};

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
}

impl Tones {
    pub fn from_theme(theme: &Theme) -> Tones {
        if theme.is_dark() {
            Tones {
                window: Color::rgba(15, 19, 27, 255),
                tab_bar: Color::rgba(18, 23, 33, 255),
                toolbar: Color::rgba(22, 27, 38, 255),
                sidebar: Color::rgba(18, 23, 33, 255),
                content: Color::rgba(15, 19, 27, 255),
                card: Color::rgba(25, 31, 43, 255),
                elevated: Color::rgba(31, 38, 52, 255),
                status_bar: Color::rgba(18, 23, 33, 255),
                selected: Color::rgba(61, 78, 123, 185),
                muted: Color::rgba(151, 162, 184, 255),
                preview_well: Color::rgba(11, 14, 21, 255),
            }
        } else {
            Tones {
                window: Color::rgba(244, 247, 252, 255),
                tab_bar: Color::rgba(235, 240, 248, 255),
                toolbar: Color::rgba(249, 251, 255, 255),
                sidebar: Color::rgba(239, 243, 250, 255),
                content: Color::rgba(249, 251, 255, 255),
                card: Color::rgba(255, 255, 255, 255),
                elevated: Color::rgba(255, 255, 255, 255),
                status_bar: Color::rgba(239, 243, 250, 255),
                selected: Color::rgba(212, 224, 255, 220),
                muted: Color::rgba(95, 108, 132, 255),
                preview_well: Color::rgba(239, 243, 250, 255),
            }
        }
    }
}

pub fn branded_theme(dark: bool) -> Theme {
    let theme = if dark { Theme::dark() } else { Theme::light() };
    if dark {
        theme
            .with_bg(Color::rgba(15, 19, 27, 255))
            .with_fg(Color::rgba(234, 239, 248, 255))
            .with_accent(Color::rgba(124, 156, 255, 255))
            .with_border(Color::rgba(255, 255, 255, 31))
            .with_hover(Color::rgba(255, 255, 255, 18))
            .with_active(Color::rgba(78, 100, 158, 180))
            .with_corner_radius(8.0)
            .with_border_width(1.0)
            .with_active_indicator_width(3.0)
            .with_font_size(14.0)
    } else {
        theme
            .with_bg(Color::rgba(249, 251, 255, 255))
            .with_fg(Color::rgba(28, 37, 53, 255))
            .with_accent(Color::rgba(72, 104, 220, 255))
            .with_border(Color::rgba(30, 45, 72, 31))
            .with_hover(Color::rgba(55, 83, 150, 15))
            .with_active(Color::rgba(205, 219, 255, 230))
            .with_corner_radius(8.0)
            .with_border_width(1.0)
            .with_active_indicator_width(3.0)
            .with_font_size(14.0)
    }
}

pub fn label_colored(frame: &mut Frame, text: &str, color: Color) {
    let theme = frame.theme();
    frame.set_theme(theme.with_fg(color));
    frame.label(text);
    frame.set_theme(theme);
}

pub fn label_colored_sized(frame: &mut Frame, text: &str, size: f32, color: Color) {
    let theme = frame.theme();
    frame.set_theme(theme.with_fg(color));
    frame.label_sized(text, size);
    frame.set_theme(theme);
}
