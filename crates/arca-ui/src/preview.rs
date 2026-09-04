//! Animated Space-bar Quick Look overlay.

use iris::{Align, Band, Frame, Input, LayoutOpts, PlaceMode, PlaceOpts, Rect};
use arca_core::{AppState, Preview, PreviewKind};

use crate::icons::{self, ids};
use crate::theme::{self, Tones};

pub(crate) struct PreviewOverlay {
    data: Option<Preview>,
    target_open: bool,
    progress: f32,
}

impl PreviewOverlay {
    pub fn new() -> PreviewOverlay {
        PreviewOverlay {
            data: None,
            target_open: false,
            progress: 0.0,
        }
    }

    pub fn toggle(&mut self, state: &mut AppState) {
        if self.target_open {
            self.target_open = false;
            return;
        }
        let Some(path) = state.selected_path() else {
            state.set_status("Select an item to preview".into());
            return;
        };
        self.data = Some(Preview::load(&path));
        self.target_open = true;
    }

    pub fn close(&mut self) {
        self.target_open = false;
    }

    pub fn reset(&mut self) {
        self.data = None;
        self.target_open = false;
        self.progress = 0.0;
    }

    pub fn sync_selection(&mut self, state: &AppState) {
        if !self.target_open {
            return;
        }
        let Some(path) = state.selected_path() else {
            self.target_open = false;
            return;
        };
        if self.data.as_ref().map(|preview| preview.path.as_str()) != Some(path.as_str()) {
            self.data = Some(Preview::load(&path));
        }
    }

    fn advance(&mut self, input: &Input, reduced_motion: bool) {
        let target = if self.target_open { 1.0 } else { 0.0 };
        if reduced_motion {
            self.progress = target;
        } else {
            let dt = input.as_raw().dt_seconds.clamp(0.0, 1.0 / 20.0);
            let speed = if self.target_open { 7.5 } else { 9.5 };
            self.progress += (target - self.progress) * (dt * speed).min(1.0);
            if (self.progress - target).abs() < 0.004 {
                self.progress = target;
            }
        }
        if (self.progress - target).abs() > f32::EPSILON {
            iris::request_animation_frame();
        } else if self.progress == 0.0 && !self.target_open {
            self.data = None;
        }
    }
}

pub(crate) fn build_preview(
    preview: &mut PreviewOverlay,
    thumbs: &mut crate::thumbs::ThumbStore,
    frame: &mut Frame,
    input: &Input,
    tones: &Tones,
) {
    let reduced = unsafe { lens_sys::lens_reduced_motion(frame.as_raw()) };
    preview.advance(input, reduced);
    if preview.progress <= 0.0 {
        return;
    }
    let Some(data) = preview.data.clone() else {
        return;
    };

    let display = input.as_raw().display_size;
    let ease = 1.0 - (1.0 - preview.progress).powi(3);
    let final_width = (display.x * 0.64).clamp(520.0, 780.0);
    let final_height = (display.y * 0.76).clamp(400.0, 640.0);
    let scale = 0.94 + 0.06 * ease;
    let width = final_width * scale;
    let height = final_height * scale;
    let x = (display.x - width) * 0.5;
    let y = (display.y - height) * 0.5 + (1.0 - ease) * 28.0;

    let scrim_alpha = tones.scrim.components().3;
    frame.place(
        "preview-backdrop",
        &PlaceOpts {
            band: Band::Modal,
            mode: PlaceMode::Exact,
            rect: Rect {
                x: 0.0,
                y: 0.0,
                w: display.x,
                h: display.y,
            },
            layout: LayoutOpts {
                bg: tones
                    .scrim
                    .with_alpha((scrim_alpha as f32 * ease).round() as u8),
                ..Default::default()
            },
            transient: false,
            ..Default::default()
        },
        |_| {},
    );

    frame.place(
        "quick-look",
        &PlaceOpts {
            band: Band::Modal,
            mode: PlaceMode::Exact,
            rect: Rect {
                x,
                y,
                w: width,
                h: height,
            },
            layout: LayoutOpts {
                gap: 0.0,
                pad: 0.0,
                cross: Align::Stretch,
                bg: tones.elevated,
                border: frame.theme().border(),
                border_width: 1.0,
                radius: 18.0,
                min_width: width,
                ..Default::default()
            },
            transient: false,
            ..Default::default()
        },
        |frame| {
            frame.size_next(width, height);
            frame.column_ex(
                &LayoutOpts {
                    width,
                    height,
                    cross: Align::Stretch,
                    ..Default::default()
                },
                |frame| {
                    build_header(preview, frame, tones, &data);
                    frame.flex(1.0);
                    build_body(thumbs, frame, tones, &data, width, height - 132.0);
                    build_footer(frame, tones, &data);
                },
            );
        },
    );
}

fn build_header(preview: &mut PreviewOverlay, frame: &mut Frame, tones: &Tones, data: &Preview) {
    frame.size_next(0.0, 76.0);
    frame.row_ex(
        &LayoutOpts {
            height: 76.0,
            gap: 12.0,
            pad: 16.0,
            cross: Align::Center,
            bg: tones.card,
            ..Default::default()
        },
        |frame| {
            icons::icon(frame, preview_icon(data.kind), 30.0);
            frame.flex(1.0);
            frame.column_ex(
                &LayoutOpts {
                    gap: 3.0,
                    ..Default::default()
                },
                |frame| {
                    frame.heading(&data.title, 3);
                    theme::label_colored_sized(frame, &data.subtitle, 12.0, tones.muted);
                },
            );
            if icons::icon_button(frame, ids::X, 32.0) {
                preview.close();
            }
        },
    );
}

fn build_body(
    thumbs: &mut crate::thumbs::ThumbStore,
    frame: &mut Frame,
    tones: &Tones,
    data: &Preview,
    width: f32,
    height: f32,
) {
    let facts_width = 190.0;
    frame.size_next(width, height);
    frame.row_ex(
        &LayoutOpts {
            width,
            height,
            cross: Align::Stretch,
            bg: tones.elevated,
            ..Default::default()
        },
        |frame| {
            frame.flex(1.0);
            frame.column_ex(
                &LayoutOpts {
                    flex: 1.0,
                    gap: 12.0,
                    pad: 18.0,
                    cross: Align::Stretch,
                    bg: tones.preview_well,
                    ..Default::default()
                },
                |frame| {
                    if let Some(text) = &data.text {
                        frame.flex(1.0);
                        frame.scroll("preview-text", |frame| {
                            frame.column_ex(
                                &LayoutOpts {
                                    pad: 10.0,
                                    cross: Align::Stretch,
                                    ..Default::default()
                                },
                                |frame| {
                                    frame.label_wrapped_sized(
                                        text,
                                        13.0,
                                        width - facts_width - 72.0,
                                    )
                                },
                            );
                        });
                    } else {
                        frame.flex(1.0);
                        frame.column_ex(
                            &LayoutOpts {
                                flex: 1.0,
                                gap: 16.0,
                                pad: 28.0,
                                cross: Align::Center,
                                ..Default::default()
                            },
                            |frame| {
                                // Album cover / the picture itself when the
                                // thumbnail store has it decoded; the kind
                                // glyph otherwise.
                                let art = matches!(data.kind, PreviewKind::Audio | PreviewKind::Image)
                                    .then(|| thumbs.image_for(&data.path))
                                    .flatten();
                                match art {
                                    Some(image) => {
                                        let max_w = (width - facts_width - 72.0).max(120.0);
                                        let max_h = (height - 56.0).max(120.0);
                                        // SAFETY: the frame is live; the store
                                        // owns the image.
                                        let (w, h) = unsafe {
                                            (
                                                flux_sys::flux_image_width(image),
                                                flux_sys::flux_image_height(image),
                                            )
                                        };
                                        let scale = (max_w / w.max(1) as f32)
                                            .min(max_h / h.max(1) as f32)
                                            .min(2.0);
                                        let (dw, dh) = (w as f32 * scale, h as f32 * scale);
                                        frame.size_next(dw, dh);
                                        // SAFETY: as above.
                                        unsafe { frame.image(image, dw, dh) };
                                    }
                                    None => icons::icon(frame, preview_icon(data.kind), 92.0),
                                }
                                theme::label_colored(frame, &data.subtitle, tones.muted);
                            },
                        );
                    }
                },
            );

            frame.size_next(facts_width, height);
            frame.column_ex(
                &LayoutOpts {
                    width: facts_width,
                    height,
                    gap: 14.0,
                    pad: 18.0,
                    cross: Align::Stretch,
                    bg: tones.card,
                    ..Default::default()
                },
                |frame| {
                    theme::label_colored_sized(frame, "DETAILS", 11.0, tones.muted);
                    for (label, value) in &data.facts {
                        frame.column_ex(
                            &LayoutOpts {
                                gap: 2.0,
                                cross: Align::Stretch,
                                ..Default::default()
                            },
                            |frame| {
                                theme::label_colored_sized(frame, label, 11.0, tones.muted);
                                frame.label_wrapped_sized(value, 13.0, facts_width - 36.0);
                            },
                        );
                    }
                },
            );
        },
    );
}

fn build_footer(frame: &mut Frame, tones: &Tones, data: &Preview) {
    frame.size_next(0.0, 52.0);
    frame.row_ex(
        &LayoutOpts {
            height: 52.0,
            gap: 12.0,
            pad: 14.0,
            cross: Align::Center,
            bg: tones.card,
            ..Default::default()
        },
        |frame| {
            frame.flex(1.0);
            theme::label_colored_sized(frame, &data.path, 11.0, tones.muted);
            theme::label_colored_sized(frame, "SPACE  Close preview", 11.0, tones.muted);
        },
    );
}

fn preview_icon(kind: PreviewKind) -> icons::AssetId {
    match kind {
        PreviewKind::Directory => ids::Folder,
        PreviewKind::Text => ids::FileText,
        PreviewKind::Image => ids::Image,
        PreviewKind::Audio => ids::Music,
        PreviewKind::Video => ids::Film,
        PreviewKind::Archive => ids::Archive,
        PreviewKind::Generic => ids::File,
    }
}
