//! Animated Space-bar Quick Look overlay.

use arca_engine::{AppState, Preview, PreviewKind};
use optics_motion::{Spring, SpringParams};
use optics_ui::{Align, Band, Frame, Input, LayoutOpts, PlaceMode, PlaceOpts, Rect};

use crate::icons::{self, ids};
use crate::theme::{self, Tones};

const PREVIEW_SPRING: SpringParams = SpringParams::new(320.0, 1.0);

pub(crate) struct PreviewOverlay {
    pub(crate) data: Option<Preview>,
    pub(crate) target_open: bool,
    pub(crate) spring: Spring,
    pub(crate) progress: f32,
    /// Whether this presentation has already been registered in optics-ui's
    /// transient open-set.
    registered: bool,
}

impl PreviewOverlay {
    pub fn new() -> PreviewOverlay {
        PreviewOverlay {
            data: None,
            target_open: false,
            spring: Spring::new(0.0),
            progress: 0.0,
            registered: false,
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

    /// Start the close animation.
    ///
    /// The optics-ui transient entry is closed by the next `build_preview` (it
    /// only exists while the node is on screen), so no frame handle is
    /// needed here.
    pub fn close(&mut self) {
        self.target_open = false;
        self.registered = false;
    }

    pub fn reset(&mut self) {
        self.data = None;
        self.target_open = false;
        self.registered = false;
        self.spring = Spring::new(0.0);
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
        let dt = input.as_raw().dt_seconds;
        self.spring
            .advance_substep(target, PREVIEW_SPRING, dt, 1.0 / 120.0, reduced_motion);
        if self.spring.settled(target, 0.002, 0.02) {
            self.spring.snap_to(target);
        }
        self.progress = self.spring.value.clamp(0.0, 1.0);
        if !self.spring.settled(target, 0.001, 0.001) {
            optics_window::request_animation_frame();
        } else if self.spring.value <= 0.0 && !self.target_open {
            self.data = None;
        }
    }
}

pub(crate) fn frame_reduced_motion(frame: &mut Frame) -> bool {
    // SAFETY: the frame is live inside the build callback.
    unsafe { optics_ui_sys::optics_ui_reduced_motion(frame.as_raw()) }
}

pub(crate) fn build_preview(
    preview: &mut PreviewOverlay,
    thumbs: &mut crate::thumbs::ThumbStore,
    frame: &mut Frame,
    input: &Input,
    tones: &Tones,
) {
    let reduced = frame_reduced_motion(frame);
    preview.advance(input, reduced);
    if preview.progress <= 0.0 {
        return;
    }
    let Some(data) = preview.data.clone() else {
        return;
    };

    // Quick Look is a dismissable modal: registering it in optics-ui's transient
    // open-set is what makes Escape and a press outside the card close it.
    // The open must happen exactly once per presentation — it is not a
    // per-frame "keep alive" — because optics-ui removes the slot on dismissal and
    // re-opening here would silently resurrect the overlay.
    if !preview.registered {
        frame.place_open(PREVIEW_OVERLAY_ID);
        preview.registered = true;
    }

    let display = input.as_raw().display_size;
    let ease = preview.progress;
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
        PREVIEW_OVERLAY_ID,
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
            transient: true,
            ..Default::default()
        },
        |frame| {
            let prev_opacity = frame.opacity();
            frame.set_opacity(ease);
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
            frame.set_opacity(prev_opacity);
        },
    );

    // The user pressed Escape or clicked outside: optics-ui removed the transient
    // without telling us, so mirror that dismissal into the app state and
    // animate out. `close()` clears `registered`, so the next presentation
    // registers afresh.
    if preview.registered && !frame.place_is_open(PREVIEW_OVERLAY_ID) {
        preview.close();
    }
}

/// Transient id of the Quick Look card (see `optics_ui_place_open`).
const PREVIEW_OVERLAY_ID: &str = "quick-look";

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
                                let art =
                                    matches!(data.kind, PreviewKind::Audio | PreviewKind::Image)
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
                                                optics_gfx_sys::optics_gfx_image_width(image),
                                                optics_gfx_sys::optics_gfx_image_height(image),
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
        PreviewKind::Special => ids::Terminal,
        PreviewKind::Generic => ids::File,
    }
}

#[cfg(test)]
mod tests {
    //! Behaviour tests for this module.
    //!
    //! Migrated out of the former monolithic `lib.rs::tests` block so each
    //! test lives next to the code it constrains.

    use crate::test_support::{fixture, frame, open_preview, press};

    #[test]
    fn preview_motion_reduced_motion_snaps_instantly() {
        let (_dir, mut app) = fixture();
        let mut ui = optics_ui::Ui::headless().expect("headless ui");
        ui.set_reduced_motion(true);

        app.state.move_selection(1);
        let mut space = optics_ui::Input::new((1040.0, 700.0), 1.0 / 60.0);
        space.push_key(' ' as i32, true, false);
        frame(&mut app, &mut ui, &space);

        // Under reduced motion, optics-motion resolves to 1.0 in a single step.
        assert_eq!(app.preview.progress, 1.0);

        // Close preview
        let mut close_space = optics_ui::Input::new((1040.0, 700.0), 1.0 / 60.0);
        close_space.push_key(' ' as i32, true, false);
        frame(&mut app, &mut ui, &close_space);

        // Under reduced motion, closing resolves to 0.0 in a single step.
        assert_eq!(app.preview.progress, 0.0);
        assert!(app.preview.data.is_none());
    }

    /// Quick Look is documented as dismissing on `Esc`; it was pinned open
    /// because the placement was not registered as a transient.
    #[test]
    fn escape_closes_quick_look() {
        let (_dir, mut app) = fixture();
        let mut ui = optics_ui::Ui::headless().expect("headless ui");
        open_preview(&mut app, &mut ui);

        press(&mut app, &mut ui, optics_ui::key::ESCAPE, 0);

        let settle = optics_ui::Input::new((1040.0, 700.0), 1.0 / 60.0);
        for _ in 0..90 {
            frame(&mut app, &mut ui, &settle);
        }
        assert!(!app.preview.target_open, "Esc must close Quick Look");
        assert_eq!(
            app.preview.progress, 0.0,
            "the overlay must animate fully out and release its data"
        );
        assert!(app.preview.data.is_none(), "preview data must be released");
    }

    /// Quick Look is documented as dismissing on a click outside the card.
    #[test]
    fn clicking_outside_closes_quick_look() {
        let (_dir, mut app) = fixture();
        let mut ui = optics_ui::Ui::headless().expect("headless ui");
        open_preview(&mut app, &mut ui);

        let mut click = optics_ui::Input::new((1040.0, 700.0), 1.0 / 60.0);
        click.set_cursor(4.0, 4.0);
        click.set_mouse_down(optics_ui::MouseButton::Left, true);
        click.set_mouse_pressed(optics_ui::MouseButton::Left, true);
        frame(&mut app, &mut ui, &click);
        let mut release = optics_ui::Input::new((1040.0, 700.0), 1.0 / 60.0);
        release.set_cursor(4.0, 4.0);
        release.set_mouse_released(optics_ui::MouseButton::Left, true);
        frame(&mut app, &mut ui, &release);

        let settle = optics_ui::Input::new((1040.0, 700.0), 1.0 / 60.0);
        for _ in 0..90 {
            frame(&mut app, &mut ui, &settle);
        }
        assert!(!app.preview.target_open, "an outside press must dismiss");
        assert_eq!(app.preview.progress, 0.0, "the overlay must animate out");
    }
}
