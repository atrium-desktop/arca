//! `UiApp`: UI-only state and the per-frame composition.

use std::time::Instant;

use arca_engine::chooser::{FileChooserMode, PromptColorScheme};
use arca_engine::config::{ThemeMode, ViewMode};
use arca_engine::state::AppState;
use iris::{Align, Frame, Input, LayoutOpts, Rect, TextBuf};

use crate::chooser::{self, ChooserState};
use crate::icons::ids;
use crate::menus;
use crate::preview::PreviewOverlay;
use crate::theme::{self, Tones};
use crate::thumbs::ThumbStore;
use crate::{content, preview, sidebar, statusbar, tabs, toolbar};

pub(crate) const CTX_MENU_ID: &str = "arca-ctx";
pub(crate) const SETTINGS_MENU_ID: &str = "arca-settings";
pub(crate) const SIDEBAR_MENU_ID: &str = "arca-sidebar-ctx";
const DOUBLE_CLICK_MS: u128 = 400;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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
    /// Pre-resolved "Open with" applications snapshot once when opening the menu
    /// so per-frame rendering does not re-query disk or reorder items.
    pub open_with: Vec<arca_xdg::desktop::DesktopEntry>,
}

pub(crate) struct SidebarMenu {
    pub path: String,
    pub is_folder: bool,
    pub anchor: Rect,
    pub opened: bool,
}

#[derive(Debug, Clone)]
pub(crate) struct ActiveDrag {
    pub name: String,
    pub path: String,
    pub is_dir: bool,
    /// The full interchange payload this drag carries — every selected item,
    /// not just the row under the pointer. Dropping must transfer the whole
    /// selection, and the payload must be the same format an external drag
    /// delivers.
    pub pending_payload: String,
    pub press_pos: (f32, f32),
    pub started: bool,
}

#[derive(Debug, Clone)]
pub(crate) struct DropTargetZone {
    pub rect: Rect,
    pub target_path: String,
    pub is_bookmark_zone: bool,
}

pub(crate) fn rect_contains(r: &Rect, p: (f32, f32)) -> bool {
    p.0 >= r.x && p.0 <= r.x + r.w && p.1 >= r.y && p.1 <= r.y + r.h
}

#[derive(Debug, Clone)]
pub(crate) struct LocationCompletionState {
    pub original_input: String,
    pub last_input: String,
    pub candidates: Vec<String>,
    pub candidate_idx: usize,
}

pub struct UiApp {
    pub state: AppState,

    /// The single-instance endpoint, when this process owns one. Held here so
    /// forwarded paths drain from this window's own mailbox and the socket is
    /// unlinked when the app is dropped.
    pub(crate) ipc: Option<crate::ipc::IpcServer>,

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

    pub(crate) pending_focus: Option<FocusTarget>,
    pub(crate) last_click: Option<(usize, Instant)>,
    pub(crate) ctx_menu: Option<CtxMenu>,
    pub(crate) settings_menu: Option<CtxMenu>,
    pub(crate) sidebar_menu: Option<SidebarMenu>,
    pub(crate) sidebar_width: f32,
    pub(crate) viewport: (f32, f32),
    pub(crate) miller_seeded_for: Option<String>,
    pub(crate) thumbs: ThumbStore,
    pub(crate) observed_tab_id: u64,
    pub(crate) preview: PreviewOverlay,
    pub chooser: Option<ChooserState>,
    pub(crate) location_completion: Option<LocationCompletionState>,
    pub(crate) active_drag: Option<ActiveDrag>,
    pub(crate) drop_targets: Vec<DropTargetZone>,
    pub(crate) cursor_pos: (f32, f32),
    pub(crate) bookmark_drop_rect: Option<Rect>,
    pub(crate) sub_row_rect: Option<Rect>,
    pub(crate) pending_paste: bool,
    pub(crate) mods: u32,
    pub(crate) fullscreen: bool,
    /// Set when the user asks to close the window (last tab closed). The host
    /// window call happens at the end of the frame so state stays coherent
    /// while the rest of the tree still builds.
    pub(crate) close_requested: bool,
    pub(crate) hovered_grid_index: Option<usize>,
    pub(crate) next_hovered_grid_index: Option<usize>,
    pub(crate) current_grid_columns: usize,
}

pub(crate) fn set_focus(frame: &mut Frame, id: u64) {
    // SAFETY: the frame is live inside the build callback.
    unsafe { lens_sys::lens_set_focus(frame.as_raw(), id) };
}

impl UiApp {
    /// Build the app around an IPC endpoint. Callers that do not own the
    /// single-instance socket (every test, the chooser windows) pass `None`.
    pub fn new(state: AppState) -> UiApp {
        UiApp::with_ipc(state, None)
    }

    pub fn with_ipc(mut state: AppState, ipc: Option<crate::ipc::IpcServer>) -> UiApp {
        state.set_fs_event_listener(Some(std::sync::Arc::new(|| {
            iris::post_to_main_thread(|| {});
        })));
        let location = TextBuf::new(4096, state.cwd());
        let filter = TextBuf::new(512, state.filter());
        let observed_tab_id = state.active_tab_id();
        UiApp {
            state,
            ipc,
            location,
            location_id: 0,
            location_focused: false,
            location_focused_now: false,
            location_completion: None,
            active_drag: None,
            drop_targets: Vec::new(),
            cursor_pos: (0.0, 0.0),
            bookmark_drop_rect: None,
            sub_row_rect: None,
            pending_paste: false,
            mods: 0,
            fullscreen: false,
            close_requested: false,
            hovered_grid_index: None,
            next_hovered_grid_index: None,
            current_grid_columns: 0,
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

    /// Shutdown callback invoked from iris lifecycle stop hook (ADR-0045).
    /// Releases GPU-backed thumbnail textures cleanly before device teardown.
    pub fn shutdown(&mut self) {
        self.thumbs.release_all();
    }

    pub fn build(&mut self, frame: &mut Frame, input: &Input) {
        let display = input.as_raw().display_size;
        self.viewport = (display.x, display.y);
        self.cursor_pos = (input.as_raw().cursor.x, input.as_raw().cursor.y);
        self.hovered_grid_index = self.next_hovered_grid_index.take();
        self.mods = input.as_raw().mods;
        let mouse_down = input.as_raw().mouse_down[0];

        // Resolve the modifier before borrowing the drag, so the immutable
        // `drop_action()` call cannot overlap the mutable drag borrow.
        let drop_action = self.drop_action();
        if let Some(drag) = &mut self.active_drag {
            if mouse_down {
                let dx = self.cursor_pos.0 - drag.press_pos.0;
                let dy = self.cursor_pos.1 - drag.press_pos.1;
                if (dx * dx + dy * dy) >= 196.0 {
                    drag.started = true;
                }
            } else {
                if drag.started {
                    let cursor = self.cursor_pos;
                    let target = self
                        .drop_targets
                        .iter()
                        .find(|z| rect_contains(&z.rect, cursor))
                        .cloned();
                    if let Some(zone) = target {
                        if zone.is_bookmark_zone {
                            self.state.add_bookmark(&drag.path);
                        } else {
                            self.state.drop_into(
                                &drag.pending_payload,
                                &zone.target_path,
                                drop_action,
                            );
                        }
                    }
                }
                self.active_drag = None;
                frame.place_close("drag-ghost-preview");
            }
        }
        self.drop_targets.clear();

        self.apply_theme(frame);
        if self.pending_paste {
            if let Some(text) = frame.take_paste() {
                self.pending_paste = false;
                self.state.paste_text(&text);
            }
        }
        self.handle_keys(frame, input);
        let forwarded = self
            .ipc
            .as_ref()
            .map(crate::ipc::IpcServer::drain)
            .unwrap_or_default();
        for path in forwarded {
            self.new_tab_at(&path);
            iris::window_restore();
            iris::window_focus();
        }
        let fs_changed = self.state.drain_fs_events();
        let job_changed = self.state.poll_async_jobs();
        // A running transfer must keep painting so progress advances and the
        // completion event is observed without waiting for the next input.
        if fs_changed || job_changed || self.state.is_transferring() {
            iris::request_animation_frame();
        }
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
        self.build_drag_preview(frame, &tones);
        self.finish_frame(frame);
    }

    /// The transfer action a drop should perform.
    ///
    /// The drag source advertises `COPY | MOVE`, but Wayland's negotiated
    /// action is not readable back through lens (`dnd_drop_info.action`
    /// echoes the *accepted* mask), so the conventional modifier decides:
    /// hold `Ctrl` to copy, drag plainly to move.
    pub(crate) fn drop_action(&self) -> arca_engine::TransferAction {
        if self.mods & lens::mods::CTRL != 0 {
            arca_engine::TransferAction::Copy
        } else {
            arca_engine::TransferAction::Move
        }
    }

    pub(crate) fn start_drag_candidate(
        &mut self,
        name: String,
        path: String,
        is_dir: bool,
        pending_payload: String,
    ) {
        if self.active_drag.is_none() {
            self.active_drag = Some(ActiveDrag {
                name,
                path,
                is_dir,
                pending_payload,
                press_pos: self.cursor_pos,
                started: false,
            });
        }
    }

    fn build_drag_preview(&self, frame: &mut Frame, tones: &Tones) {
        let Some(drag) = &self.active_drag else {
            return;
        };
        if !drag.started {
            return;
        }

        let preview_id = "drag-ghost-preview";
        frame.place_open(preview_id);
        frame.place(
            preview_id,
            &lens::PlaceOpts {
                band: lens::Band::Tooltip,
                mode: lens::PlaceMode::Exact,
                rect: Rect {
                    x: self.cursor_pos.0 + 12.0,
                    y: self.cursor_pos.1 + 12.0,
                    w: 0.0,
                    h: 0.0,
                },
                interactive: false,
                layout: LayoutOpts {
                    gap: 6.0,
                    pad: 6.0,
                    cross: Align::Center,
                    bg: iris::Color::rgba(22, 27, 40, 225),
                    border: tones.selected,
                    border_width: 1.0,
                    radius: 7.0,
                    ..Default::default()
                },
                ..Default::default()
            },
            |frame| {
                frame.row_ex(
                    &LayoutOpts {
                        gap: 6.0,
                        cross: Align::Center,
                        ..Default::default()
                    },
                    |frame| {
                        let icon_id = if drag.is_dir { ids::Folder } else { ids::File };
                        crate::icons::icon(frame, icon_id, 16.0);
                        let fg = frame.theme().fg();
                        theme::label_colored_sized(frame, &drag.name, 12.0, fg);
                    },
                );
            },
        );
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
        if self.close_requested {
            iris::window_close();
        }
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
                set_focus(frame, id);
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
        self.location_completion = None;
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
        // The engine keeps the final tab (a window must never enter an
        // invalid no-document state), so closing the last tab is a window
        // close — which is what the shortcut documentation promises.
        if self.state.tabs().len() == 1 {
            self.close_requested = true;
            return;
        }
        if self.state.close_active_tab() {
            self.sync_active_tab();
        }
    }

    pub(crate) fn close_tab(&mut self, index: usize) {
        if self.state.close_tab(index) {
            self.sync_active_tab();
        }
    }

    pub(crate) fn move_tab(&mut self, from: usize, to: usize) {
        if self.state.move_tab(from, to) {
            self.sync_active_tab();
        }
    }

    pub(crate) fn toggle_preview(&mut self) {
        self.preview.toggle(&mut self.state);
    }

    pub(crate) fn grid_columns(&self) -> usize {
        if self.current_grid_columns > 0 {
            return self.current_grid_columns;
        }
        let available = (self.viewport.0 - self.sidebar_width - 20.0).max(120.0);
        let plan = lens::patterns::virtual_grid_calc(
            available,
            self.viewport.1,
            0.0,
            100,
            content::GRID_MIN_CARD_WIDTH,
            content::GRID_MAX_CARD_WIDTH,
            content::GRID_DEFAULT_CARD_HEIGHT,
            content::GRID_GAP,
            2,
        );
        plan.columns.max(1) as usize
    }

    pub(crate) fn focus_location(&mut self) {
        self.location_completion = None;
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

    pub(crate) fn commit_rename(&mut self, frame: &mut Frame) {
        if let Some(original) = self.renaming.take() {
            let new_name = self.rename.as_str().into_owned();
            self.state.rename(&original, &new_name);
        }
        frame.clear_focus();
    }

    pub(crate) fn submit_location(&mut self, frame: &mut Frame) {
        self.location_completion = None;
        let dest = self.location.as_str().into_owned();
        let before = self.state.cwd().to_string();
        self.state.navigate(&dest);
        if self.state.cwd() != before {
            self.location.set(self.state.cwd());
            self.location_focused = false;
            self.location_focused_now = false;
            frame.clear_focus();
        }
    }

    pub(crate) fn complete_location(&mut self, frame: &mut Frame) {
        frame.consume_key(lens::key::TAB);
        self.pending_focus = Some(FocusTarget::Location);
        let current = self.location.as_str().into_owned();

        if let Some(state) = &mut self.location_completion {
            if !state.candidates.is_empty()
                && (current == state.last_input || state.candidates.contains(&current))
            {
                let next_idx = (state.candidate_idx + 1) % state.candidates.len();
                let next = state.candidates[next_idx].clone();
                state.candidate_idx = next_idx;
                self.location.set(&next);
                frame.textfield_set_caret("##location", next.len() as u32);
                self.state.set_status(format!(
                    "[{}/{}] {} (Tab to cycle)",
                    next_idx + 1,
                    state.candidates.len(),
                    next
                ));
                return;
            }
        }

        if let Some(comp) = arca_engine::complete_path(self.state.cwd(), &current) {
            if comp.completed != current {
                self.location.set(&comp.completed);
                frame.textfield_set_caret("##location", comp.completed.len() as u32);
                let cand_len = comp.candidates.len();
                if cand_len > 1 {
                    self.state.set_status(format!(
                        "{} matches: {}...",
                        cand_len,
                        comp.candidates
                            .iter()
                            .take(3)
                            .cloned()
                            .collect::<Vec<_>>()
                            .join(", ")
                    ));
                    self.location_completion = Some(LocationCompletionState {
                        original_input: current,
                        last_input: comp.completed,
                        candidates: comp.candidates,
                        candidate_idx: cand_len - 1,
                    });
                } else {
                    self.location_completion = None;
                    self.state
                        .set_status(format!("Completed: {}", comp.completed));
                }
            } else if !comp.candidates.is_empty() {
                let first = comp.candidates[0].clone();
                self.location.set(&first);
                frame.textfield_set_caret("##location", first.len() as u32);
                self.state.set_status(format!(
                    "[1/{}] {} (Tab to cycle)",
                    comp.candidates.len(),
                    first
                ));
                self.location_completion = Some(LocationCompletionState {
                    original_input: current,
                    last_input: first,
                    candidates: comp.candidates,
                    candidate_idx: 0,
                });
            }
        }
    }

    pub(crate) fn row_clicked(&mut self, visible_idx: usize, name: &str) {
        let now = Instant::now();
        let ctrl = self.mods & lens::mods::CTRL != 0;
        let shift = self.mods & lens::mods::SHIFT != 0;

        if ctrl {
            self.state.toggle_select_visible(visible_idx);
            self.last_click = Some((visible_idx, now));
            if let Some(chooser) = &mut self.chooser {
                if chooser.request.mode == FileChooserMode::OpenFile && chooser.allows_multiple() {
                    let path = std::path::Path::new(self.state.cwd()).join(name);
                    chooser.toggle_selected_path(path);
                }
            }
            return;
        }
        if shift {
            self.state.select_range_visible(visible_idx);
            self.last_click = Some((visible_idx, now));
            return;
        }

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
                            // Double-click accepts the whole staged selection
                            // in multiple mode, not just the clicked row.
                            let path = std::path::Path::new(self.state.cwd()).join(name);
                            let paths = chooser.accept_selection(Some(path));
                            chooser.accept_paths(paths);
                            iris::window_close();
                            return;
                        } else if chooser.request.mode == FileChooserMode::SaveFile {
                            chooser.save_name.set(name);
                            crate::chooser::trigger_accept(self);
                            return;
                        }
                    }
                }
                let token = iris::window_create_activation_token(None);
                self.state.open_child_with_token(name, token.as_deref());
                return;
            }
        }
        self.last_click = Some((visible_idx, now));
        self.state.select_visible(visible_idx);
        if let Some(chooser) = &mut self.chooser {
            let entry = self
                .state
                .entries()
                .iter()
                .find(|e| e.name == name)
                .map(|e| (e.navigable, e.name.clone()));
            match entry {
                // SaveFile mirrors the clicked file into the name field.
                Some((false, clicked)) if chooser.request.mode == FileChooserMode::SaveFile => {
                    chooser.save_name.set(&clicked);
                }
                // Multi-select pickers stage the row; Accept returns the set.
                Some((false, clicked))
                    if chooser.request.mode == FileChooserMode::OpenFile
                        && chooser.allows_multiple() =>
                {
                    let path = std::path::Path::new(self.state.cwd()).join(&clicked);
                    chooser.toggle_selected_path(path);
                }
                _ => {}
            }
        }
    }

    pub(crate) fn row_right_clicked(&mut self, visible_idx: usize, anchor: Rect) {
        if !self.state.is_selected(visible_idx) {
            self.state.select_visible(visible_idx);
        }
        let open_with = self
            .state
            .selected_entry()
            .filter(|e| !e.navigable)
            .map(|e| arca_xdg::desktop::applications_for_mime(&e.mime_type))
            .unwrap_or_default();
        self.ctx_menu = Some(CtxMenu {
            anchor,
            opened: false,
            open_with,
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
                open_with: Vec::new(),
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
                // Dismissal order is innermost-first, and every layer that
                // handles Escape consumes the key so lens's transient pass
                // (which runs after the build) cannot dismiss a *different*
                // overlay in the same keystroke.
                if self.renaming.is_some() {
                    frame.consume_key(lens::key::ESCAPE);
                    self.renaming = None;
                    frame.clear_focus();
                    continue;
                }
                if self.location_focused {
                    frame.consume_key(lens::key::ESCAPE);
                    if let Some(comp) = self.location_completion.take() {
                        self.location.set(&comp.original_input);
                        frame.textfield_set_caret("##location", comp.original_input.len() as u32);
                        self.state.set_status(String::new());
                    } else {
                        self.location.set(self.state.cwd());
                        frame.clear_focus();
                        self.location_focused = false;
                        self.location_focused_now = false;
                    }
                    continue;
                }
                if self.filter_focused {
                    frame.consume_key(lens::key::ESCAPE);
                    frame.clear_focus();
                    self.filter_focused = false;
                    self.filter_focused_now = false;
                    continue;
                }
                // Quick Look is a modal layer above the listing: Escape
                // closes it before anything else can react.
                if self.preview.target_open {
                    frame.consume_key(lens::key::ESCAPE);
                    self.preview.close();
                    continue;
                }
                // A long transfer is cancellable; cancellation outranks any
                // remaining dismissal layer because it is the only way to
                // stop disk work the user started.
                if self.state.is_transferring() {
                    frame.consume_key(lens::key::ESCAPE);
                    self.state.cancel_transfer();
                    continue;
                }
                if let Some(chooser) = &mut self.chooser {
                    frame.consume_key(lens::key::ESCAPE);
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
                    frame.consume_key(lens::key::RETURN);
                    if self.rename_focused {
                        self.commit_rename(frame);
                    } else if chooser_save_focused {
                        chooser::trigger_accept(self);
                    }
                } else if key == lens::key::TAB && self.location_focused {
                    frame.consume_key(lens::key::TAB);
                }
                continue;
            }

            let ctrl = mods & lens::mods::CTRL != 0;
            let alt = mods & lens::mods::ALT != 0;
            let shift = mods & lens::mods::SHIFT != 0;

            const KEY_SPACE: i32 = b' ' as i32;

            match key {
                lens::key::TAB if ctrl => {
                    frame.consume_key(lens::key::TAB);
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
                    let token = iris::window_create_activation_token(None);
                    self.state.open_selected_with_token(token.as_deref());
                }
                lens::key::UP if !ctrl && !alt => {
                    let step = if self.state.view_mode == ViewMode::Grid {
                        self.grid_columns() as i64
                    } else {
                        1
                    };
                    self.state.move_selection_ext(-step, shift);
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
                    self.state.move_selection_ext(step, shift);
                }
                lens::key::RETURN if !ctrl && !alt => {
                    if self.chooser.is_some() {
                        chooser::trigger_accept(self);
                    } else {
                        self.preview.close();
                        let token = iris::window_create_activation_token(None);
                        self.state.open_selected_with_token(token.as_deref());
                    }
                }
                lens::key::DELETE if shift && !ctrl && !alt => {
                    self.preview.close();
                    self.state.delete_selected_permanently();
                }
                lens::key::DELETE if !ctrl && !alt => {
                    self.preview.close();
                    self.state.trash_selected();
                }
                lens::key::F2 if !ctrl && !alt => {
                    if let Some(name) = self.state.selected_entry().map(|entry| entry.name.clone())
                    {
                        self.begin_rename(name);
                    }
                }
                lens::key::F3 if !ctrl && !alt => self.focus_filter(),
                lens::key::F5 if !alt => self.state.refresh(),
                lens::key::F11 if !ctrl && !alt => {
                    self.fullscreen = !self.fullscreen;
                    if self.fullscreen {
                        iris::window_fullscreen();
                    } else {
                        iris::window_windowed();
                    }
                }
                KEY_SPACE if !ctrl && !alt => self.preview.toggle(&mut self.state),
                k if ctrl => match k as u8 as char {
                    'r' if !shift => self.state.refresh(),
                    'h' if !shift => self.state.toggle_hidden(),
                    'l' if !shift => self.focus_location(),
                    'f' if !shift => self.focus_filter(),
                    't' if !shift && self.chooser.is_none() => self.new_tab(),
                    'w' if !shift && self.chooser.is_none() => self.close_active_tab(),
                    'z' if !shift => self.state.undo(),
                    'z' if shift => self.state.redo(),
                    'y' if !shift => self.state.redo(),
                    'a' if !shift => self.state.select_all(),
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
                        self.state.yank_selected(arca_engine::TransferAction::Copy);
                        if let Some(payload) = self.state.clipboard_payload() {
                            frame.copy(&payload);
                        }
                    }
                    'x' if !shift => {
                        self.state.yank_selected(arca_engine::TransferAction::Move);
                        if let Some(payload) = self.state.clipboard_payload() {
                            frame.copy(&payload);
                        }
                    }
                    'v' if !shift => {
                        if let Some(text) = frame.take_paste() {
                            self.state.paste_text(&text);
                        } else {
                            self.pending_paste = true;
                            frame.request_paste();
                            if self.state.clipboard.is_some() {
                                self.state.paste();
                                self.pending_paste = false;
                            }
                        }
                    }
                    // Standard quit chord: closing the window is the only
                    // sane meaning for it in a single-window file manager.
                    'q' if !shift => self.close_requested = true,
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

#[cfg(test)]
mod tests {
    //! Behaviour tests for this module.
    //!
    //! Migrated out of the former monolithic `lib.rs::tests` block so each
    //! test lives next to the code it constrains.

    use crate::test_support::{fixture, fixture_two_files, frame, press};
    use arca_engine::TransferAction;

    #[test]
    fn builds_frames_headless() {
        let (_dir, mut app) = fixture();
        let mut ui = lens::Ui::headless().expect("headless ui");
        let input = lens::Input::new((1040.0, 700.0), 1.0 / 60.0);
        // Several frames so the retained tree reconciles enter/stable/leave.
        for _ in 0..3 {
            frame(&mut app, &mut ui, &input);
        }
    }

    #[test]
    fn ctrl_h_toggles_hidden_files() {
        let (_dir, mut app) = fixture();
        let mut ui = lens::Ui::headless().expect("headless ui");
        assert!(!app.state.show_hidden);
        assert_eq!(app.state.entries().len(), 2);

        let mut input = lens::Input::new((1040.0, 700.0), 1.0 / 60.0);
        input.set_mods(lens::mods::CTRL);
        input.push_key('h' as i32, true, false);
        frame(&mut app, &mut ui, &input);

        assert!(app.state.show_hidden);
        assert_eq!(app.state.entries().len(), 3);
    }

    #[test]
    fn arrows_select_and_return_enters_directory() {
        let (_dir, mut app) = fixture();
        let mut ui = lens::Ui::headless().expect("headless ui");

        let mut down = lens::Input::new((1040.0, 700.0), 1.0 / 60.0);
        down.push_key(lens::key::DOWN, true, false);
        frame(&mut app, &mut ui, &down);
        assert_eq!(
            app.state.selected_entry().map(|e| e.name.as_str()),
            Some("sub"),
            "first entry is the sub directory (dirs sort first)"
        );

        let mut enter = lens::Input::new((1040.0, 700.0), 1.0 / 60.0);
        enter.push_key(lens::key::RETURN, true, false);
        frame(&mut app, &mut ui, &enter);
        assert!(app.state.cwd().ends_with("/sub"));
    }

    #[test]
    fn filter_field_filters_the_list() {
        let (_dir, mut app) = fixture();
        let mut ui = lens::Ui::headless().expect("headless ui");
        app.state.set_filter("txt".into());
        let input = lens::Input::new((1040.0, 700.0), 1.0 / 60.0);
        frame(&mut app, &mut ui, &input);
        assert_eq!(app.state.visible().len(), 1);
    }

    #[test]
    fn double_click_into_directory_does_not_panic_on_stale_rows() {
        let (_dir, mut app) = fixture();
        let mut ui = lens::Ui::headless().expect("headless ui");
        let settle = lens::Input::new((1040.0, 700.0), 1.0 / 60.0);
        for _ in 0..2 {
            frame(&mut app, &mut ui, &settle);
        }

        // Row 0 is the "sub" directory. Two quick clicks on it trigger the
        // double-click open mid-build; the rows built afterwards in the same
        // frame still reference the old (now stale) entry indices.
        let row0 = app
            .sub_row_rect
            .map(|r| (r.x + r.w * 0.5, r.y + r.h * 0.5))
            .unwrap_or((500.0, 150.0));
        for _ in 0..2 {
            let mut press = lens::Input::new((1040.0, 700.0), 1.0 / 60.0);
            press.set_cursor(row0.0, row0.1);
            press.set_mouse_down(lens::MouseButton::Left, true);
            press.set_mouse_pressed(lens::MouseButton::Left, true);
            frame(&mut app, &mut ui, &press);
            let mut release = lens::Input::new((1040.0, 700.0), 1.0 / 60.0);
            release.set_cursor(row0.0, row0.1);
            release.set_mouse_released(lens::MouseButton::Left, true);
            frame(&mut app, &mut ui, &release);
        }
        assert!(app.state.cwd().ends_with("/sub"));
        for _ in 0..3 {
            frame(&mut app, &mut ui, &settle);
        }
    }

    #[test]
    fn keyboard_shortcuts_rename_and_delete_undo() {
        let (_dir, mut app) = fixture();
        let mut ui = lens::Ui::headless().expect("headless ui");
        let settle = lens::Input::new((1040.0, 700.0), 1.0 / 60.0);
        frame(&mut app, &mut ui, &settle);

        // Select first item
        let mut down = lens::Input::new((1040.0, 700.0), 1.0 / 60.0);
        down.push_key(lens::key::DOWN, true, false);
        frame(&mut app, &mut ui, &down);
        assert!(app.state.selected().is_some());

        // Press Ctrl+E to begin rename
        let mut rename_in = lens::Input::new((1040.0, 700.0), 1.0 / 60.0);
        rename_in.set_mods(lens::mods::CTRL);
        rename_in.push_key('e' as i32, true, false);
        frame(&mut app, &mut ui, &rename_in);
        assert!(app.renaming.is_some());

        // Cancel rename with Esc
        let mut esc = lens::Input::new((1040.0, 700.0), 1.0 / 60.0);
        esc.push_key(lens::key::ESCAPE, true, false);
        frame(&mut app, &mut ui, &esc);
        assert!(app.renaming.is_none());

        // Delete item to trash
        let mut del = lens::Input::new((1040.0, 700.0), 1.0 / 60.0);
        del.push_key(lens::key::DELETE, true, false);
        frame(&mut app, &mut ui, &del);
        assert_eq!(app.state.entries().len(), 1);

        // Ctrl+Z to undo trash
        let mut undo = lens::Input::new((1040.0, 700.0), 1.0 / 60.0);
        undo.set_mods(lens::mods::CTRL);
        undo.push_key('z' as i32, true, false);
        frame(&mut app, &mut ui, &undo);
        assert_eq!(app.state.entries().len(), 2);
    }

    #[test]
    fn keyboard_shortcuts_function_keys() {
        let (_dir, mut app) = fixture();
        let mut ui = lens::Ui::headless().expect("headless ui");
        let settle = lens::Input::new((1040.0, 700.0), 1.0 / 60.0);
        frame(&mut app, &mut ui, &settle);

        // Select first item
        let mut down = lens::Input::new((1040.0, 700.0), 1.0 / 60.0);
        down.push_key(lens::key::DOWN, true, false);
        frame(&mut app, &mut ui, &down);
        assert!(app.state.selected().is_some());

        // Press F2 to begin rename
        let mut f2 = lens::Input::new((1040.0, 700.0), 1.0 / 60.0);
        f2.push_key(lens::key::F2, true, false);
        frame(&mut app, &mut ui, &f2);
        assert!(app.renaming.is_some());

        // Cancel rename with Esc
        let mut esc = lens::Input::new((1040.0, 700.0), 1.0 / 60.0);
        esc.push_key(lens::key::ESCAPE, true, false);
        frame(&mut app, &mut ui, &esc);
        assert!(app.renaming.is_none());

        // Press F3 to focus search/filter
        let mut f3 = lens::Input::new((1040.0, 700.0), 1.0 / 60.0);
        f3.push_key(lens::key::F3, true, false);
        frame(&mut app, &mut ui, &f3);
        frame(&mut app, &mut ui, &settle);
        assert!(app.filter_focused);

        // Escape clears filter focus
        let mut esc_filter = lens::Input::new((1040.0, 700.0), 1.0 / 60.0);
        esc_filter.push_key(lens::key::ESCAPE, true, false);
        frame(&mut app, &mut ui, &esc_filter);
        frame(&mut app, &mut ui, &settle);
        assert!(!app.filter_focused);

        // Press F5 to refresh
        let mut f5 = lens::Input::new((1040.0, 700.0), 1.0 / 60.0);
        f5.push_key(lens::key::F5, true, false);
        frame(&mut app, &mut ui, &f5);

        // Press F11 to toggle fullscreen
        assert!(!app.fullscreen);
        let mut f11 = lens::Input::new((1040.0, 700.0), 1.0 / 60.0);
        f11.push_key(lens::key::F11, true, false);
        frame(&mut app, &mut ui, &f11);
        assert!(app.fullscreen);

        let mut f11_off = lens::Input::new((1040.0, 700.0), 1.0 / 60.0);
        f11_off.push_key(lens::key::F11, true, false);
        frame(&mut app, &mut ui, &f11_off);
        assert!(!app.fullscreen);
    }

    #[test]
    fn paste_from_external_system_clipboard_uri_list() {
        let (dir, mut app) = fixture();
        let mut ui = lens::Ui::headless().expect("headless ui");
        let settle = lens::Input::new((1040.0, 700.0), 1.0 / 60.0);
        frame(&mut app, &mut ui, &settle);

        // Create an external file to copy into the sub folder
        let sub_dir = dir.join("sub");
        let external = dir.join("a.txt");
        assert!(external.exists());

        // Navigate inside sub folder
        app.state.navigate(sub_dir.to_str().unwrap());
        frame(&mut app, &mut ui, &settle);
        assert_eq!(app.state.entries().len(), 0);

        // Simulate incoming paste text from system clipboard (text/uri-list)
        let uri_payload = format!("file://{}\r\n", external.display());
        ui.paste(&uri_payload);

        let mut paste_input = lens::Input::new((1040.0, 700.0), 1.0 / 60.0);
        paste_input.set_mods(lens::mods::CTRL);
        paste_input.push_key('v' as i32, true, false);
        frame(&mut app, &mut ui, &paste_input);
        frame(&mut app, &mut ui, &settle);

        assert!(sub_dir.join("a.txt").exists());
        assert_eq!(app.state.entries().len(), 1);
    }

    #[test]
    fn ui_multi_selection_ctrl_a_and_batch_trash() {
        let (_dir, mut app) = fixture();
        let mut ui = lens::Ui::headless().expect("headless ui");
        let settle = lens::Input::new((1040.0, 700.0), 1.0 / 60.0);
        frame(&mut app, &mut ui, &settle);

        // Select all with Ctrl+A
        let mut ctrl_a = lens::Input::new((1040.0, 700.0), 1.0 / 60.0);
        ctrl_a.set_mods(lens::mods::CTRL);
        ctrl_a.push_key('a' as i32, true, false);
        frame(&mut app, &mut ui, &ctrl_a);

        assert_eq!(app.state.selected_count(), 2);
        assert_eq!(app.state.selected_paths().len(), 2);

        // Batch delete selected items to trash
        let mut del = lens::Input::new((1040.0, 700.0), 1.0 / 60.0);
        del.push_key(lens::key::DELETE, true, false);
        frame(&mut app, &mut ui, &del);

        assert_eq!(app.state.entries().len(), 0);

        // Ctrl+Z restores all items
        let mut undo = lens::Input::new((1040.0, 700.0), 1.0 / 60.0);
        undo.set_mods(lens::mods::CTRL);
        undo.push_key('z' as i32, true, false);
        frame(&mut app, &mut ui, &undo);

        assert_eq!(app.state.entries().len(), 2);
    }

    #[test]
    fn ctrl_t_opens_tab_and_space_preview_animates() {
        let (_dir, mut app) = fixture();
        let mut ui = lens::Ui::headless().expect("headless ui");

        let mut new_tab = lens::Input::new((1040.0, 700.0), 1.0 / 60.0);
        new_tab.set_mods(lens::mods::CTRL);
        new_tab.push_key('t' as i32, true, false);
        frame(&mut app, &mut ui, &new_tab);
        assert_eq!(app.state.tabs().len(), 2);

        app.state.move_selection(1);
        let mut space = lens::Input::new((1040.0, 700.0), 1.0 / 60.0);
        space.push_key(' ' as i32, true, false);
        frame(&mut app, &mut ui, &space);
        assert!(app.preview.target_open);

        let settle = lens::Input::new((1040.0, 700.0), 1.0 / 60.0);
        for _ in 0..8 {
            frame(&mut app, &mut ui, &settle);
        }
        assert!(app.preview.progress > 0.5);
    }

    #[test]
    fn location_bar_tab_completion_works() {
        let (dir, mut app) = fixture();
        let mut ui = lens::Ui::headless().expect("headless ui");
        let input = lens::Input::new((1040.0, 700.0), 1.0 / 60.0);
        for _ in 0..2 {
            frame(&mut app, &mut ui, &input);
        }

        app.focus_location();
        for _ in 0..2 {
            frame(&mut app, &mut ui, &input);
        }

        let partial = format!("{}/s", dir.to_str().unwrap());
        app.location.set(&partial);

        let mut tab = lens::Input::new((1040.0, 700.0), 1.0 / 60.0);
        tab.push_key(lens::key::TAB, true, false);
        frame(&mut app, &mut ui, &tab);

        assert_eq!(
            app.location.as_str(),
            format!("{}/sub/", dir.to_str().unwrap())
        );

        // Test multiple matches: first Tab completes up to common prefix "folder_"
        std::fs::create_dir_all(dir.join("folder_a")).unwrap();
        std::fs::create_dir_all(dir.join("folder_b")).unwrap();
        let prefix = format!("{}/fol", dir.to_str().unwrap());
        app.location.set(&prefix);
        app.location_completion = None;

        // First Tab: advances to longest common prefix "folder_"
        frame(&mut app, &mut ui, &tab);
        assert_eq!(
            app.location.as_str(),
            format!("{}/folder_", dir.to_str().unwrap())
        );

        // Second Tab: cycles to first candidate "folder_a/"
        frame(&mut app, &mut ui, &tab);
        assert_eq!(
            app.location.as_str(),
            format!("{}/folder_a/", dir.to_str().unwrap())
        );

        // Third Tab: cycles to second candidate "folder_b/"
        frame(&mut app, &mut ui, &tab);
        assert_eq!(
            app.location.as_str(),
            format!("{}/folder_b/", dir.to_str().unwrap())
        );

        // Fourth Tab: wraps back to first candidate "folder_a/"
        frame(&mut app, &mut ui, &tab);
        assert_eq!(
            app.location.as_str(),
            format!("{}/folder_a/", dir.to_str().unwrap())
        );
    }

    #[test]
    fn ui_updates_file_list_on_external_fs_change() {
        let (dir, mut app) = fixture();
        if app.state.watcher.is_none() {
            eprintln!("inotify instances exhausted on host; skipping test");
            return;
        }
        let mut ui = lens::Ui::headless().expect("headless ui");
        let input = lens::Input::new((1040.0, 700.0), 1.0 / 60.0);
        frame(&mut app, &mut ui, &input);
        assert_eq!(app.state.entries().len(), 2); // sub, a.txt

        // External process downloads a file
        std::thread::sleep(std::time::Duration::from_millis(50));
        let new_file = dir.join("external_download.iso");
        std::fs::write(&new_file, b"iso contents").unwrap();

        let start = std::time::Instant::now();
        let mut updated = false;
        while start.elapsed() < std::time::Duration::from_secs(2) {
            frame(&mut app, &mut ui, &input);
            if app
                .state
                .entries()
                .iter()
                .any(|e| e.name == "external_download.iso")
            {
                updated = true;
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(20));
        }

        assert!(
            updated,
            "expected UI to pick up external file download on subsequent frames"
        );
    }

    #[test]
    fn app_shutdown_cleans_up_resources() {
        let (_dir, mut app) = fixture();
        app.shutdown();
        assert!(crate::device::device().is_none());
    }

    #[test]
    fn filter_textfield_hover_cursor_hint() {
        let mut ui = lens::Ui::headless().expect("headless ui");
        let mut tf_buf = lens::TextBuf::new(64, "test");
        let input = lens::Input::new((400.0, 200.0), 1.0 / 60.0);

        // Frame 1: resolve layout so node has prev_rect
        ui.frame(&input, |f| {
            f.size_next(200.0, 36.0);
            f.textfield_placeholder("##tf", &mut tf_buf, "placeholder");
        });
        if let Ok(snap) = ui.snapshot() {
            let _ = ui.activate(&snap);
        }

        // Frame 2: hover cursor over the textfield at (50, 18)
        let mut hover_input = lens::Input::new((400.0, 200.0), 1.0 / 60.0);
        hover_input.set_cursor(50.0, 18.0);
        ui.frame(&hover_input, |f| {
            f.size_next(200.0, 36.0);
            f.textfield_placeholder("##tf", &mut tf_buf, "placeholder");
        });

        assert_eq!(ui.cursor_hint(), lens::CursorHint::Text);
    }

    /// Deleting the last remaining tab is documented as exiting the app; the
    /// engine refuses, so the UI must close the window explicitly rather than
    /// silently doing nothing.
    #[test]
    fn ctrl_w_on_the_last_tab_requests_window_close() {
        let (_dir, mut app) = fixture();
        let mut ui = lens::Ui::headless().expect("headless ui");
        let settle = lens::Input::new((1040.0, 700.0), 1.0 / 60.0);
        frame(&mut app, &mut ui, &settle);
        assert_eq!(app.state.tabs().len(), 1);

        press(&mut app, &mut ui, 'w' as i32, lens::mods::CTRL);

        // The engine keeps the final tab (never an invalid no-document
        // state), and the UI asks the host to close the window instead.
        assert_eq!(app.state.tabs().len(), 1);
        assert!(
            app.close_requested,
            "closing the last tab must request a window close"
        );
    }

    /// `Ctrl+Q` is the conventional quit chord; it must request a window
    /// close rather than being advertised in the reference and doing nothing.
    #[test]
    fn ctrl_q_requests_window_close() {
        let (_dir, mut app) = fixture();
        let mut ui = lens::Ui::headless().expect("headless ui");
        let settle = lens::Input::new((1040.0, 700.0), 1.0 / 60.0);
        frame(&mut app, &mut ui, &settle);
        assert!(!app.close_requested);

        press(&mut app, &mut ui, 'q' as i32, lens::mods::CTRL);
        assert!(app.close_requested, "Ctrl+Q must request a window close");
    }

    /// A re-read of the directory (Ctrl+R, or any inotify event) used to
    /// leave the cursor index pointing at a row that was no longer marked
    /// selected — the highlight vanished.
    #[test]
    fn refresh_keeps_the_selection_highlight() {
        let (_dir, mut app) = fixture();
        let mut ui = lens::Ui::headless().expect("headless ui");
        let settle = lens::Input::new((1040.0, 700.0), 1.0 / 60.0);
        frame(&mut app, &mut ui, &settle);

        app.state.select_visible(1);
        let selected_name = app.state.selected_entry().map(|e| e.name.clone());
        assert_eq!(selected_name.as_deref(), Some("a.txt"));
        assert!(app.state.is_selected(1));

        app.state.refresh();
        assert_eq!(
            app.state.selected_entry().map(|e| e.name.clone()),
            selected_name,
            "the cursor must stay on the same file"
        );
        assert_eq!(app.state.selected_count(), 1);
        assert!(
            app.state.is_selected(1),
            "the row must still be highlighted"
        );
    }

    /// Sorting reorders the listing; the selection must follow the file, not
    /// stay on the old row index.
    #[test]
    fn sorting_keeps_the_selection_on_the_same_file() {
        let (_dir, mut app) = fixture();
        let mut ui = lens::Ui::headless().expect("headless ui");
        let settle = lens::Input::new((1040.0, 700.0), 1.0 / 60.0);
        frame(&mut app, &mut ui, &settle);

        app.state.select_visible(1);
        assert_eq!(
            app.state
                .selected_entry()
                .map(|e| e.name.clone())
                .as_deref(),
            Some("a.txt")
        );

        app.state.toggle_sort(arca_engine::SortKey::Size);
        frame(&mut app, &mut ui, &settle);

        assert_eq!(
            app.state
                .selected_entry()
                .map(|e| e.name.clone())
                .as_deref(),
            Some("a.txt"),
            "the selection must follow the file across a re-sort"
        );
        assert_eq!(app.state.selected_count(), 1);
    }

    /// A single-item drag from the listing must produce an interchange
    /// payload that the engine can parse — not a bare filesystem path that
    /// only happens to work when nothing else is selected.
    #[test]
    fn in_app_drag_payload_round_trips_through_the_engine() {
        let (dir, mut app) = fixture();
        let mut ui = lens::Ui::headless().expect("headless ui");
        let settle = lens::Input::new((1040.0, 700.0), 1.0 / 60.0);
        frame(&mut app, &mut ui, &settle);

        let source = dir.join("a.txt");
        let payload =
            arca_xdg::FileClipboard::new(vec![source.clone()], TransferAction::Move).to_uri_list();
        let destination = dir.join("sub");

        assert!(app.state.drop_into(
            &payload,
            destination.to_str().unwrap(),
            TransferAction::Move
        ));
        assert!(!source.exists());
        assert!(destination.join("a.txt").exists());
    }

    /// A multi-item drag must transfer the whole selection.
    #[test]
    fn multi_selection_drag_transfers_every_item() {
        let (dir, mut app) = fixture_two_files();
        let mut ui = lens::Ui::headless().expect("headless ui");
        let settle = lens::Input::new((1040.0, 700.0), 1.0 / 60.0);
        frame(&mut app, &mut ui, &settle);

        app.state.select_visible(1);
        app.state.select_range_visible(2);
        assert_eq!(app.state.selected_count(), 2);
        let paths: Vec<std::path::PathBuf> = app
            .state
            .selected_paths()
            .into_iter()
            .map(std::path::PathBuf::from)
            .collect();
        assert_eq!(paths.len(), 2);

        let payload = arca_xdg::FileClipboard::new(paths, TransferAction::Move).to_uri_list();
        let destination = dir.join("sub");
        assert!(app.state.drop_into(
            &payload,
            destination.to_str().unwrap(),
            TransferAction::Move
        ));

        assert!(destination.join("a.txt").exists());
        assert!(destination.join("b.txt").exists());
        assert!(!dir.join("a.txt").exists());
        assert!(!dir.join("b.txt").exists());
    }

    /// A running background transfer must be cancellable, and the status bar
    /// must be able to observe it — the worker existed but was never driven.
    #[test]
    fn large_transfer_runs_off_thread_and_can_be_cancelled() {
        let (dir, mut app) = fixture();
        let mut ui = lens::Ui::headless().expect("headless ui");
        let settle = lens::Input::new((1040.0, 700.0), 1.0 / 60.0);
        frame(&mut app, &mut ui, &settle);

        // Larger than the async threshold (8 MiB).
        let big = dir.join("big.bin");
        std::fs::write(&big, vec![0x7eu8; 12 * 1024 * 1024]).unwrap();
        std::fs::create_dir_all(dir.join("target")).unwrap();

        app.state.navigate(dir.to_str().unwrap());
        app.state.clipboard = Some(arca_engine::Clipboard::new(
            vec![big.clone()],
            TransferAction::Copy,
        ));
        app.state.navigate(dir.join("target").to_str().unwrap());
        app.state.paste();

        assert!(
            app.state.is_transferring(),
            "a 12 MiB copy must be handed to the worker, status: {:?}",
            app.state.status
        );

        // The frame loop keeps painting while the job runs.
        frame(&mut app, &mut ui, &settle);
        assert!(big.exists(), "a copy keeps the source");

        app.state.cancel_transfer();

        // Drain until the completion event lands.
        for _ in 0..400 {
            frame(&mut app, &mut ui, &settle);
            if !app.state.is_transferring() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        assert!(
            !app.state.is_transferring(),
            "cancellation must finish the job"
        );
    }

    #[test]
    fn patterns_datagrid_and_treeview_headless() {
        let mut ui = lens::Ui::headless().expect("headless ui");
        let input = lens::Input::new((800.0, 600.0), 1.0 / 60.0);

        let mut cols = vec![
            iris::patterns::DataGridCol::new("Name").width(200.0),
            iris::patterns::DataGridCol::new("Size").width(100.0),
            iris::patterns::DataGridCol::new("Modified").width(150.0),
        ];
        let cells = [
            "file1.txt",
            "1.2 KB",
            "Today",
            "file2.rs",
            "4.5 KB",
            "Yesterday",
        ];
        let mut selected_row = 0;

        let mut tree_items = vec![
            iris::patterns::TreeItem::new("Root Folder").expanded(true),
            iris::patterns::TreeItem::new("Documents")
                .depth(1)
                .expanded(false),
            iris::patterns::TreeItem::new("file.txt")
                .depth(2)
                .leaf(true),
        ];

        ui.frame(&input, |f| {
            // DataGrid pattern
            let grid_act = iris::patterns::DataGrid::new("test_files_grid")
                .striped(true)
                .show(f, &mut cols, &cells, 2, &mut selected_row);
            assert_eq!(grid_act, iris::patterns::DataGridAction::None);

            // TreeView pattern
            let tree_act =
                iris::patterns::TreeView::new("test_files_tree").show(f, &mut tree_items);
            assert_eq!(tree_act, iris::patterns::TreeAction::None);
        });

        assert!(!ui.overflowed());
    }
}
