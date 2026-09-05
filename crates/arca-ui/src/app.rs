//! `UiApp`: UI-only state and the per-frame composition.

use std::time::Instant;

use iris::{Align, Frame, Input, LayoutOpts, Rect, TextBuf};
use arca_engine::chooser::{FileChooserMode, PromptColorScheme};
use arca_engine::config::{ThemeMode, ViewMode};
use arca_engine::state::AppState;

use crate::chooser::{self, ChooserState};
use crate::menus;
use crate::preview::PreviewOverlay;
use crate::theme::{self, Tones};
use crate::thumbs::ThumbStore;
use crate::{content, preview, sidebar, statusbar, tabs, toolbar};

pub(crate) const CTX_MENU_ID: &str = "arca-ctx";
pub(crate) const SETTINGS_MENU_ID: &str = "arca-settings";
pub(crate) const SIDEBAR_MENU_ID: &str = "arca-sidebar-ctx";
const DOUBLE_CLICK_MS: u128 = 400;

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum FocusTarget {
    Location,
    Filter,
    Rename,
}

pub(crate) struct CtxMenu {
    pub anchor: Rect,
    /// Overlay ids hash per id-scope, so the open/begin/close calls must all
    /// happen in the menu-building scope — a trigger deep in the widget tree
    /// only records state here, and the builder performs the first open.
    pub opened: bool,
}

pub(crate) struct SidebarMenu {
    pub path: String,
    pub is_folder: bool,
    pub anchor: Rect,
    pub opened: bool,
}

pub struct UiApp {
    pub state: AppState,

    pub(crate) location: TextBuf,
    pub(crate) location_id: lens_sys::lens_id,
    pub(crate) location_focused: bool,
    pub(crate) location_focused_now: bool,

    pub(crate) filter: TextBuf,
    pub(crate) filter_id: lens_sys::lens_id,
    pub(crate) filter_focused: bool,
    pub(crate) filter_focused_now: bool,

    pub(crate) renaming: Option<String>,
    pub(crate) rename: TextBuf,
    pub(crate) rename_id: lens_sys::lens_id,
    pub(crate) rename_focused: bool,
    pub(crate) rename_focused_now: bool,

    pending_focus: Option<FocusTarget>,
    pub(crate) last_click: Option<(usize, Instant)>,
    pub(crate) ctx_menu: Option<CtxMenu>,
    pub(crate) settings_menu: Option<CtxMenu>,
    pub(crate) sidebar_menu: Option<SidebarMenu>,
    pub(crate) sidebar_width: f32,
    pub(crate) viewport: (f32, f32),
    pub(crate) miller_seeded_for: Option<String>,
    pub(crate) thumbs: ThumbStore,
    observed_tab_id: u64,
    preview: PreviewOverlay,
    pub chooser: Option<ChooserState>,
}

impl UiApp {
    pub fn new(state: AppState) -> UiApp {
        let location = TextBuf::new(4096, state.cwd());
        let filter = TextBuf::new(512, state.filter());
        let observed_tab_id = state.active_tab_id();
        UiApp {
            state,
            location,
            location_id: 0,
            location_focused: false,
            location_focused_now: false,
            filter,
            filter_id: 0,
            filter_focused: false,
            filter_focused_now: false,
            renaming: None,
            rename: TextBuf::new(1024, ""),
            rename_id: 0,
            rename_focused: false,
            rename_focused_now: false,
            pending_focus: None,
            last_click: None,
            ctx_menu: None,
            settings_menu: None,
            sidebar_menu: None,
            sidebar_width: 220.0,
            viewport: (1040.0, 700.0),
            miller_seeded_for: None,
            thumbs: ThumbStore::new(),
            observed_tab_id,
            preview: PreviewOverlay::new(),
            chooser: None,
        }
    }

    pub fn build(&mut self, frame: &mut Frame, input: &Input) {
        let display = input.as_raw().display_size;
        self.viewport = (display.x, display.y);
        self.apply_theme(frame);
        self.handle_keys(frame, input);
        self.sync_active_tab();
        self.thumbs.sync_cwd(self.state.cwd());
        self.thumbs.set_enabled(self.state.show_thumbnails);
        self.thumbs.drain_uploads();

        let tones = Tones::from_theme(&frame.theme());
        frame.size_next(display.x, display.y);
        frame.column_ex(
            &LayoutOpts {
                width: display.x,
                height: display.y,
                cross: Align::Stretch,
                bg: tones.window,
                ..Default::default()
            },
            |frame| {
                if self.chooser.is_some() {
                    chooser::build_chooser_header(self, frame, input, &tones);
                } else {
                    tabs::build_tabs(self, frame, input, &tones);
                }
                toolbar::build_toolbar(self, frame, &tones);

                frame.flex(1.0);
                frame.row().show(|frame| {
                    sidebar::build_sidebar(self, frame, &tones);
                    content::build_content(self, frame, &tones);
                });

                if self.chooser.is_some() {
                    chooser::build_chooser_footer(self, frame, &tones);
                } else {
                    statusbar::build_statusbar(self, frame, &tones);
                }
            },
        );

        if self.chooser.is_some() {
            chooser::build_filter_popover(self, frame, &tones);
            chooser::build_overwrite_modal(self, frame, &tones);
        } else {
            menus::build_ctx_menu(self, frame, &tones);
            menus::build_settings_menu(self, frame, &tones);
            menus::build_sidebar_menu(self, frame, &tones);
            self.preview.sync_selection(&self.state);
            preview::build_preview(&mut self.preview, &mut self.thumbs, frame, input, &tones);
        }
        self.finish_frame(frame);
    }

    fn apply_theme(&mut self, frame: &mut Frame) {
        if let Some(chooser) = &self.chooser {
            if let Some(appr) = &chooser.appearance {
                let dark = match appr.color_scheme {
                    PromptColorScheme::System => iris::system_prefers_dark(),
                    PromptColorScheme::Light => false,
                    PromptColorScheme::Dark => true,
                };
                frame.set_theme(theme::branded_theme_with_accent(dark, appr.accent_color));
                return;
            }
        }
        let dark = match self.state.theme {
            ThemeMode::System => iris::system_prefers_dark(),
            ThemeMode::Light => false,
            ThemeMode::Dark => true,
        };
        frame.set_theme(theme::branded_theme(dark));
    }

    fn finish_frame(&mut self, frame: &mut Frame) {
        if self.renaming.is_some() && self.rename_focused && !self.rename_focused_now {
            self.renaming = None;
        }

        self.location_focused = self.location_focused_now;
        self.filter_focused = self.filter_focused_now;
        self.rename_focused = self.rename_focused_now;

        if let Some(chooser) = &mut self.chooser {
            chooser.save_name_focused = chooser.save_name_focused_now;
        }

        if let Some(target) = self.pending_focus {
            let id = match target {
                FocusTarget::Location => self.location_id,
                FocusTarget::Filter => self.filter_id,
                FocusTarget::Rename => self.rename_id,
            };
            if id != 0 {
                unsafe { lens_sys::lens_set_focus(frame.as_raw(), id) };
                self.pending_focus = None;
            }
        }
    }

    fn sync_active_tab(&mut self) {
        let id = self.state.active_tab_id();
        if id == self.observed_tab_id {
            return;
        }
        self.observed_tab_id = id;
        self.location.set(self.state.cwd());
        self.filter.set(self.state.filter());
        self.location_focused = false;
        self.filter_focused = false;
        self.rename_focused = false;
        self.renaming = None;
        self.pending_focus = None;
        self.last_click = None;
        self.ctx_menu = None;
        self.settings_menu = None;
        self.miller_seeded_for = None;
        self.preview.reset();
    }

    pub(crate) fn switch_tab(&mut self, index: usize) {
        if self.state.switch_tab(index) {
            self.sync_active_tab();
        }
    }

    pub(crate) fn new_tab(&mut self) {
        self.state.new_tab(None);
        self.sync_active_tab();
    }

    pub(crate) fn new_tab_at(&mut self, path: &str) {
        self.state.new_tab(Some(path));
        self.sync_active_tab();
    }

    pub(crate) fn close_active_tab(&mut self) {
        if self.state.close_active_tab() {
            self.sync_active_tab();
        }
    }

    pub(crate) fn close_tab(&mut self, index: usize) {
        if self.state.close_tab(index) {
            self.sync_active_tab();
        }
    }

    pub(crate) fn toggle_preview(&mut self) {
        self.preview.toggle(&mut self.state);
    }

    pub(crate) fn grid_columns(&self) -> usize {
        let available = (self.viewport.0 - self.sidebar_width - 20.0).max(160.0);
        let plan = lens::patterns::virtual_grid_calc(
            available,
            self.viewport.1,
            0.0,
            100,
            136.0,
            176.0,
            120.0,
            12.0,
            2,
        );
        plan.columns.max(1) as usize
    }

    pub(crate) fn focus_location(&mut self) {
        self.location.set("");
        self.pending_focus = Some(FocusTarget::Location);
    }

    pub(crate) fn focus_filter(&mut self) {
        self.pending_focus = Some(FocusTarget::Filter);
    }

    pub(crate) fn begin_rename(&mut self, name: String) {
        self.rename.set(&name);
        self.renaming = Some(name);
        self.rename_id = 0;
        self.rename_focused = false;
        self.rename_focused_now = false;
        self.pending_focus = Some(FocusTarget::Rename);
    }

    fn commit_rename(&mut self, frame: &mut Frame) {
        if let Some(original) = self.renaming.take() {
            let new_name = self.rename.as_str().into_owned();
            self.state.rename(&original, &new_name);
        }
        frame.clear_focus();
    }

    fn submit_location(&mut self, frame: &mut Frame) {
        let dest = self.location.as_str().into_owned();
        let before = self.state.cwd().to_string();
        self.state.navigate(&dest);
        if self.state.cwd() != before {
            frame.clear_focus();
        }
    }

    pub(crate) fn row_clicked(&mut self, visible_idx: usize, name: &str) {
        let now = Instant::now();
        if let Some((last_idx, at)) = self.last_click {
            if last_idx == visible_idx && now.duration_since(at).as_millis() < DOUBLE_CLICK_MS {
                self.last_click = None;
                self.preview.close();
                if let Some(chooser) = &mut self.chooser {
                    let entry = self.state.entries().iter().find(|e| e.name == name);
                    if let Some(entry) = entry {
                        if entry.navigable {
                            self.state.open_child(name);
                            return;
                        } else if chooser.request.mode == FileChooserMode::OpenFile {
                            let path = std::path::Path::new(self.state.cwd()).join(name);
                            chooser.accept_paths(vec![path]);
                            iris::window_close();
                            return;
                        } else if chooser.request.mode == FileChooserMode::SaveFile {
                            chooser.save_name.set(name);
                            crate::chooser::trigger_accept(self);
                            return;
                        }
                    }
                }
                self.state.open_child(name);
                return;
            }
        }
        self.last_click = Some((visible_idx, now));
        self.state.select_visible(visible_idx);
        if let Some(chooser) = &mut self.chooser {
            if chooser.request.mode == FileChooserMode::SaveFile {
                let entry = self.state.entries().iter().find(|e| e.name == name);
                if let Some(entry) = entry {
                    if !entry.navigable {
                        chooser.save_name.set(name);
                    }
                }
            }
        }
    }

    pub(crate) fn row_right_clicked(&mut self, visible_idx: usize, anchor: Rect) {
        self.state.select_visible(visible_idx);
        self.ctx_menu = Some(CtxMenu {
            anchor,
            opened: false,
        });
    }

    pub(crate) fn open_sidebar_menu(&mut self, path: &str, is_folder: bool, anchor: Rect) {
        self.sidebar_menu = Some(SidebarMenu {
            path: path.to_string(),
            is_folder,
            anchor,
            opened: false,
        });
    }

    pub(crate) fn toggle_settings_menu(&mut self, anchor: Rect) {
        self.settings_menu = match self.settings_menu.take() {
            Some(_) => None,
            None => Some(CtxMenu {
                anchor,
                opened: false,
            }),
        };
    }

    pub(crate) fn miller_clicked(&mut self, directory: &str, name: &str) {
        self.preview.close();
        self.state.activate_miller_item(directory, name);
        self.last_click = None;
    }

    fn handle_keys(&mut self, frame: &mut Frame, input: &Input) {
        let raw = input.as_raw();
        let mods = raw.mods;
        let chooser_save_focused = self
            .chooser
            .as_ref()
            .map(|c| c.save_name_focused)
            .unwrap_or(false);
        let text_editing = self.location_focused
            || self.filter_focused
            || self.rename_focused
            || chooser_save_focused;

        for ev in &raw.keys[..raw.key_count as usize] {
            if !ev.pressed {
                continue;
            }
            let key = ev.key;

            if key == lens::key::ESCAPE {
                if let Some(chooser) = &mut self.chooser {
                    if chooser.overwrite_confirm.is_some() {
                        chooser.overwrite_confirm = None;
                    } else if chooser.filter_menu_open {
                        chooser.filter_menu_open = false;
                    } else {
                        chooser.cancel();
                        iris::window_close();
                    }
                    continue;
                }
            }

            if text_editing {
                if key == lens::key::RETURN {
                    if self.rename_focused {
                        self.commit_rename(frame);
                    } else if self.location_focused {
                        self.submit_location(frame);
                    } else if chooser_save_focused {
                        chooser::trigger_accept(self);
                    }
                }
                continue;
            }

            let ctrl = mods & lens::mods::CTRL != 0;
            let alt = mods & lens::mods::ALT != 0;
            let shift = mods & lens::mods::SHIFT != 0;

            match key {
                lens::key::TAB if ctrl => {
                    self.state.cycle_tab(shift);
                    self.sync_active_tab();
                }
                lens::key::LEFT if alt => self.state.go_back(),
                lens::key::RIGHT if alt => self.state.go_forward(),
                lens::key::UP if alt => self.state.go_up(),
                lens::key::BACKSPACE if !ctrl && !alt => self.state.go_up(),
                lens::key::LEFT if !ctrl && !alt && self.state.view_mode == ViewMode::Grid => {
                    self.state.move_selection(-1)
                }
                lens::key::RIGHT if !ctrl && !alt && self.state.view_mode == ViewMode::Grid => {
                    self.state.move_selection(1)
                }
                lens::key::LEFT if !ctrl && !alt && self.state.view_mode == ViewMode::Miller => {
                    self.state.go_up()
                }
                lens::key::RIGHT if !ctrl && !alt && self.state.view_mode == ViewMode::Miller => {
                    self.preview.close();
                    self.state.open_selected();
                }
                lens::key::UP if !ctrl && !alt => {
                    let step = if self.state.view_mode == ViewMode::Grid {
                        self.grid_columns() as i64
                    } else {
                        1
                    };
                    self.state.move_selection(-step);
                }
                lens::key::DOWN if !ctrl && !alt => {
                    let step = if self.state.view_mode == ViewMode::Grid {
                        if self.state.selected().is_none() {
                            1
                        } else {
                            self.grid_columns() as i64
                        }
                    } else {
                        1
                    };
                    self.state.move_selection(step);
                }
                lens::key::RETURN if !ctrl && !alt => {
                    if self.chooser.is_some() {
                        chooser::trigger_accept(self);
                    } else {
                        self.preview.close();
                        self.state.open_selected();
                    }
                }
                lens::key::DELETE if !ctrl && !alt => {
                    self.preview.close();
                    self.state.trash_selected();
                }
                32 if !ctrl && !alt => self.preview.toggle(&mut self.state),
                k if ctrl => match k as u8 as char {
                    'r' if !shift => self.state.refresh(),
                    'h' if !shift => self.state.toggle_hidden(),
                    'l' if !shift => self.focus_location(),
                    'f' if !shift => self.focus_filter(),
                    't' if !shift && self.chooser.is_none() => self.new_tab(),
                    'w' if !shift && self.chooser.is_none() => self.close_active_tab(),
                    'n' if shift => {
                        if let Some(name) = self.state.create_folder() {
                            self.begin_rename(name);
                        }
                    }
                    'e' if !shift => {
                        if let Some(name) =
                            self.state.selected_entry().map(|entry| entry.name.clone())
                        {
                            self.begin_rename(name);
                        }
                    }
                    'c' if !shift => {
                        self.state.yank_selected(false);
                        if let Some(payload) = self.state.clipboard_payload() {
                            frame.copy(&payload);
                        }
                    }
                    'x' if !shift => {
                        self.state.yank_selected(true);
                        if let Some(payload) = self.state.clipboard_payload() {
                            frame.copy(&payload);
                        }
                    }
                    'v' if !shift => self.state.paste(),
                    '1'..='9' if !shift && self.chooser.is_none() => {
                        self.switch_tab((k as u8 - b'1') as usize)
                    }
                    _ => {}
                },
                _ => {}
            }
        }
    }
}
