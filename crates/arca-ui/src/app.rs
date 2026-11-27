//! `UiApp`: UI-only state and the per-frame composition.

use std::time::Instant;

use arca_engine::chooser::FileChooserMode;
use arca_engine::config::{ThemeMode, ViewMode};
use arca_engine::state::AppState;
use optics_ui::{Align, CornerRadii, Frame, Input, LayoutOpts, Rect, TextBuf, Ui};

use crate::chooser::{self, ChooserState};
use crate::icons::{IconSet, ids};
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

    /// Show Warning/Critical telemetry alerts in the status bar. Only the
    /// interactive window opts in ([`Self::surface_alerts`]); headless tests
    /// keep it off so alerts raised by sibling tests cannot leak into their
    /// status assertions.
    pub(crate) alerts_in_status: bool,

    /// The single-instance endpoint, when this process owns one. Held here so
    /// forwarded paths drain from this window's own mailbox and the socket is
    /// unlinked when the app is dropped.
    pub(crate) ipc: Option<crate::ipc::IpcServer>,

    pub(crate) location: TextBuf,
    pub(crate) location_focused: bool,
    pub(crate) location_focused_now: bool,

    pub(crate) filter: TextBuf,
    pub(crate) filter_focused: bool,
    pub(crate) filter_focused_now: bool,

    pub(crate) renaming: Option<String>,
    pub(crate) rename: TextBuf,
    pub(crate) rename_focused: bool,
    pub(crate) rename_focused_now: bool,

    pub(crate) pending_focus: Option<FocusTarget>,
    /// Last clicked entry (absolute path) and time. Identity is the path, not
    /// the visible index or the bare name: navigation, filtering and sorting
    /// rebuild the index space, and two directories can each contain a `sub`.
    /// An index-only check turned "click a file, navigate, click the row at the
    /// same position" into a spurious double-click that launched the wrong
    /// entry.
    pub(crate) last_click: Option<(String, Instant)>,
    pub(crate) ctx_menu: Option<CtxMenu>,
    pub(crate) settings_menu: Option<CtxMenu>,
    pub(crate) sidebar_menu: Option<SidebarMenu>,
    pub(crate) sidebar_width: f32,
    pub(crate) viewport: (f32, f32),
    pub(crate) miller_seeded_for: Option<String>,
    /// Preview directory whose column scroll was last reset to the top.
    pub(crate) miller_preview_seeded_for: Option<String>,
    pub(crate) thumbs: ThumbStore,
    pub(crate) observed_tab_id: u64,
    /// Per-MIME "Open with" snapshot. Resolving an association scans and
    /// parses every installed `.desktop` file; doing that synchronously on
    /// every right-click turns repeated clicking into a disk-latency storm on
    /// large or slow application directories. The set is immutable for the
    /// window's lifetime (installing an application is not an interaction this
    /// cache must observe).
    pub(crate) open_with_cache:
        std::collections::HashMap<String, Vec<arca_xdg::desktop::DesktopEntry>>,
    pub(crate) preview: PreviewOverlay,
    pub chooser: Option<ChooserState>,
    pub(crate) location_completion: Option<LocationCompletionState>,
    pub(crate) active_drag: Option<ActiveDrag>,
    pub(crate) drop_targets: Vec<DropTargetZone>,
    pub(crate) cursor_pos: (f32, f32),
    pub(crate) bookmark_drop_rect: Option<Rect>,
    pub(crate) sub_row_rect: Option<Rect>,
    pub(crate) pending_paste: bool,
    /// The reduced-motion policy last applied to optics-ui's caret blink clock.
    /// `None` forces one application after context creation.
    pub(crate) caret_blink_reduced: Option<bool>,
    pub(crate) mods: u32,
    pub(crate) fullscreen: bool,
    /// Set when the user asks to close the window (last tab closed). The host
    /// window call happens at the end of the frame so state stays coherent
    /// while the rest of the tree still builds.
    pub(crate) close_requested: bool,
    pub(crate) hovered_grid_index: Option<usize>,
    pub(crate) next_hovered_grid_index: Option<usize>,
    pub(crate) current_grid_columns: usize,
    /// Whether navigation reads happen on a worker. Production enables this;
    /// deterministic tests and the chooser opt into synchronous reads.
    pub(crate) async_navigation: bool,
    /// Arca's glyphs, registered on the `optics_ui` context that builds this
    /// app's frames ([`UiApp::start`]). Ids are only meaningful on that
    /// context; until registration every glyph is `OPTICS_UI_ICON_NONE`.
    pub(crate) icons: IconSet,
    /// Test-only observation point: an activation token must never be requested
    /// merely to navigate into a directory (the request can block the UI thread
    /// until the compositor answers).
    #[cfg(test)]
    pub(crate) activation_token_requests: usize,
}

/// Move keyboard focus to the first widget built under one of `labels`.
///
/// optics-ui resolves the label against the current id scope, so this must run
/// immediately after the widget inside the container that built it; a label
/// resolved from the root scope cannot address a nested field. Storing a
/// label instead of a numeric widget id keeps focus independent of the id
/// that an earlier build happened to assign. Returns `false` when none of the
/// labels has been built, which leaves `pending_focus` armed for the next
/// frame.
pub(crate) fn focus_any(frame: &mut Frame, labels: &[&str]) -> bool {
    for label in labels {
        let name = *label;
        let Ok(label) = std::ffi::CString::new(name) else {
            continue;
        };
        // SAFETY: the frame is live inside the build callback and `label` is
        // NUL-terminated. `optics_ui_current_id` only reads the context's id map.
        let id = unsafe { optics_ui_sys::optics_ui_current_id(frame.as_raw(), label.as_ptr()) };
        if id == 0 {
            continue;
        }
        // SAFETY: the frame is live and `id` was resolved against this context.
        unsafe { optics_ui_sys::optics_ui_set_focus(frame.as_raw(), id) };
        return true;
    }
    false
}

/// Release keyboard focus from whatever widget holds it.
pub(crate) fn clear_focus(frame: &mut Frame) {
    // SAFETY: the frame is live inside the build callback; 0 is "no widget".
    unsafe { optics_ui_sys::optics_ui_set_focus(frame.as_raw(), 0) };
}

impl UiApp {
    /// Build the app around an IPC endpoint. Callers that do not own the
    /// single-instance socket (every test, the chooser windows) pass `None`.
    pub fn new(state: AppState) -> UiApp {
        UiApp::assemble(state, None, None)
    }

    pub fn with_ipc(state: AppState, ipc: Option<crate::ipc::IpcServer>) -> UiApp {
        UiApp::assemble(state, ipc, None)
    }

    /// Build the app with an explicit thumbnail cache directory.
    ///
    /// The default cache is the user's FreeDesktop thumbnail store, and its
    /// service starts a janitor that prunes old entries. Tests and embedders
    /// must pass a scratch path here instead of mutating the real cache.
    pub fn with_thumb_cache(state: AppState, cache_dir: impl Into<std::path::PathBuf>) -> UiApp {
        UiApp::assemble(state, None, Some(cache_dir.into()))
    }

    /// Force synchronous directory reads.
    ///
    /// Deterministic headless tests and the chooser use this; the interactive
    /// window keeps the default asynchronous navigation so a slow directory
    /// never blocks the frame that received the click.
    pub fn sync_navigation(mut self) -> UiApp {
        self.async_navigation = false;
        self
    }

    /// Toggle asynchronous navigation after construction (test/embedding).
    pub fn set_async_navigation(&mut self, enabled: bool) {
        self.async_navigation = enabled;
    }

    /// Route Warning/Critical telemetry alerts (UI stalls, render-loop spins)
    /// into the status bar. The interactive window enables this.
    pub fn surface_alerts(mut self) -> UiApp {
        self.alerts_in_status = true;
        self
    }

    fn show_alert_notice(&mut self) {
        if !self.alerts_in_status {
            return;
        }
        if let Some(alert) = arca_engine::telemetry::alerts::global().take_notice() {
            // Direct assignment: the alert was already logged when raised.
            self.state.status = Some(format!("⚠ {}", alert.summary));
        }
    }

    fn assemble(
        mut state: AppState,
        ipc: Option<crate::ipc::IpcServer>,
        thumb_cache: Option<std::path::PathBuf>,
    ) -> UiApp {
        state.set_fs_event_listener(Some(std::sync::Arc::new(|| {
            optics_window::post_to_main_thread(|| {});
        })));
        let mut thumbs = match thumb_cache {
            Some(cache_dir) => ThumbStore::with_cache_dir(cache_dir),
            None => ThumbStore::new(),
        };
        // A finished decode wakes the event loop so the card repaints with
        // its thumbnail instead of waiting for the next pointer motion.
        thumbs.set_wake_fn(Some(std::sync::Arc::new(|| {
            optics_window::post_to_main_thread(|| {});
        })));
        let location = TextBuf::new(4096, state.cwd());
        let filter = TextBuf::new(512, state.filter());
        let observed_tab_id = state.active_tab_id();
        UiApp {
            state,
            alerts_in_status: false,
            ipc,
            location,
            location_focused: false,
            location_focused_now: false,
            location_completion: None,
            active_drag: None,
            drop_targets: Vec::new(),
            cursor_pos: (0.0, 0.0),
            bookmark_drop_rect: None,
            sub_row_rect: None,
            pending_paste: false,
            caret_blink_reduced: None,
            mods: 0,
            fullscreen: false,
            close_requested: false,
            hovered_grid_index: None,
            next_hovered_grid_index: None,
            current_grid_columns: 0,
            async_navigation: true,
            icons: IconSet::EMPTY,
            #[cfg(test)]
            activation_token_requests: 0,
            filter,
            filter_focused: false,
            filter_focused_now: false,
            renaming: None,
            rename: TextBuf::new(1024, ""),
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
            miller_preview_seeded_for: None,
            thumbs,
            observed_tab_id,
            open_with_cache: std::collections::HashMap::new(),
            preview: PreviewOverlay::new(),
            chooser: None,
        }
    }

    /// Start callback invoked from the optics-app lifecycle start hook, once
    /// the window has created `ui` and before the first frame: registers the
    /// icon set on that context (Optics ADR-0174, ADR-0025).
    pub fn start(&mut self, ui: &mut Ui) {
        self.icons = IconSet::register(ui);
    }

    /// Shutdown callback invoked from the optics-app lifecycle stop hook (ADR-0045).
    /// Releases GPU-backed thumbnail textures and the context's icons cleanly
    /// before the window tears down the device and `ui`.
    pub fn shutdown(&mut self, ui: &mut Ui) {
        self.thumbs.release_all();
        self.icons.unregister(ui);
    }

    pub fn build(&mut self, frame: &mut Frame, input: &Input) {
        // The heartbeat the watchdog samples: a frame that never reaches the
        // end of this function is a stall, named by the innermost span.
        arca_engine::telemetry::span::mark_ui_thread();
        let _frame = arca_engine::telemetry::watchdog::FrameGuard::begin();
        self.show_alert_notice();
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

        self.sync_caret_blink(frame);
        self.apply_theme(frame);
        if self.pending_paste
            && let Some(text) = frame.take_paste()
        {
            self.pending_paste = false;
            self.state.paste_text(&text);
        }
        self.handle_keys(frame, input);
        let forwarded = self
            .ipc
            .as_ref()
            .map(crate::ipc::IpcServer::drain)
            .unwrap_or_default();
        for req in forwarded {
            let path = req.path.to_path_buf().to_string_lossy().into_owned();
            self.new_tab_at(&path);
            optics_window::window_restore();
            if let Some(ref token) = req.activation_token {
                // `arca-ipc-v2` forwards `XDG_ACTIVATION_TOKEN` to the UI
                // boundary; `optics-window` v0.0.90 does not yet expose
                // `xdg_activation_v1::activate(token)`, so fall back to
                // `window_focus()` until that binding lands (ADR-0023).
                log::debug!(
                    target: "arca_ui",
                    "received forwarded activation token ({} bytes); falling back to window_focus",
                    token.len()
                );
            }
            optics_window::window_focus();
        }
        let nav_changed = self.state.poll_navigation();
        let fs_changed = self.state.drain_fs_events();
        let job_changed = self.state.poll_async_jobs();
        // A running transfer or navigation must keep painting so progress
        // advances and the completion event is observed without waiting for
        // the next input.
        if nav_changed
            || self.state.is_navigating()
            || fs_changed
            || job_changed
            || self.state.is_transferring()
        {
            optics_window::request_animation_frame();
        }
        self.sync_active_tab();
        self.thumbs.sync_cwd(self.state.cwd());
        self.thumbs.set_enabled(self.state.show_thumbnails);
        self.thumbs.drain_uploads();

        let tones = Tones::from_theme(&frame.theme());
        frame.size_next(display.x, display.y);
        frame
            .column()
            .with_opts(&LayoutOpts {
                width: display.x,
                height: display.y,
                cross: Align::Stretch,
                bg: tones.window,
                ..Default::default()
            })
            .show(|frame| {
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
            });

        if self.chooser.is_some() {
            chooser::build_filter_popover(self, frame, &tones);
            chooser::build_overwrite_modal(self, frame, &tones);
        } else {
            menus::build_ctx_menu(self, frame, &tones);
            menus::build_settings_menu(self, frame, &tones);
            menus::build_sidebar_menu(self, frame, &tones);
            self.preview.sync_selection(&self.state);
            preview::build_preview(
                &mut self.preview,
                &mut self.thumbs,
                frame,
                &self.icons,
                input,
                &tones,
            );
        }
        self.build_drag_preview(frame, &tones);
        self.finish_frame();
    }

    /// The transfer action a drop should perform.
    ///
    /// The drag source advertises `COPY | MOVE`, but Wayland's negotiated
    /// action is not readable back through optics-ui (`DndDropInfo::action`
    /// echoes the *accepted* mask), so the conventional modifier decides:
    /// hold `Ctrl` to copy, drag plainly to move.
    pub(crate) fn drop_action(&self) -> arca_engine::TransferAction {
        if self.mods & optics_ui::mods::CTRL != 0 {
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
        let icons = self.icons;
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
            &optics_ui::PlaceOpts {
                band: optics_ui::Band::Tooltip,
                mode: optics_ui::PlaceMode::Exact,
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
                    bg: optics_ui::Color::rgba(22, 27, 40, 225),
                    border: tones.selected,
                    border_width: 1.0,
                    corner_radii: CornerRadii::uniform(7.0),
                    ..Default::default()
                },
                ..Default::default()
            },
            |frame| {
                frame
                    .row()
                    .with_opts(&LayoutOpts {
                        gap: 6.0,
                        cross: Align::Center,
                        ..Default::default()
                    })
                    .show(|frame| {
                        let icon_id = if drag.is_dir { ids::Folder } else { ids::File };
                        icons.icon(frame, icon_id, 16.0);
                        let fg = frame.theme().fg();
                        theme::label_colored_sized(frame, &drag.name, 12.0, fg);
                    });
            },
        );
    }

    /// Mirror the host's reduced-motion policy into optics-ui's caret blink clock
    /// (Optics ADR-0151). A reduced-motion session keeps the caret solid and
    /// silent; leaving it restores the library default through the exported
    /// constant, never a hard-coded period.
    fn sync_caret_blink(&mut self, frame: &mut Frame) {
        let reduced = preview::frame_reduced_motion(frame);
        if self.caret_blink_reduced == Some(reduced) {
            return;
        }
        self.caret_blink_reduced = Some(reduced);
        let period = if reduced {
            0.0
        } else {
            optics_ui::DEFAULT_CARET_BLINK_PERIOD_S
        };
        // SAFETY: the frame is live inside the build callback.
        unsafe { optics_ui_sys::optics_ui_set_caret_blink(frame.as_raw(), period) };
    }

    fn apply_theme(&mut self, frame: &mut Frame) {
        let dark = match self.state.theme {
            ThemeMode::System => optics_window::system_prefers_dark(),
            ThemeMode::Light => false,
            ThemeMode::Dark => true,
        };
        frame.set_theme(theme::branded_theme(dark));
    }

    fn finish_frame(&mut self) {
        if self.close_requested {
            optics_window::window_close();
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
        self.miller_preview_seeded_for = None;
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
        let plan = optics_ui::patterns::virtual_grid_calc(
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
        self.rename_focused = false;
        self.rename_focused_now = false;
        self.pending_focus = Some(FocusTarget::Rename);
    }

    pub(crate) fn commit_rename(&mut self, frame: &mut Frame) {
        if let Some(original) = self.renaming.take() {
            let new_name = self.rename.as_str().into_owned();
            self.state.rename(&original, &new_name);
        }
        clear_focus(frame);
    }

    pub(crate) fn submit_location(&mut self, frame: &mut Frame) {
        self.location_completion = None;
        let dest = self.location.as_str().into_owned();
        if self.navigate_to(&dest) {
            // For an asynchronous request the toolbar updates the location
            // text once the reader publishes the new cwd.
            self.location.set(self.state.cwd());
            self.location_focused = false;
            self.location_focused_now = false;
            clear_focus(frame);
        }
    }

    pub(crate) fn complete_location(&mut self, frame: &mut Frame) {
        frame.consume_key(optics_ui::key::TAB);
        self.pending_focus = Some(FocusTarget::Location);
        let current = self.location.as_str().into_owned();

        if let Some(state) = &mut self.location_completion
            && !state.candidates.is_empty()
            && (current == state.last_input || state.candidates.contains(&current))
        {
            let next_idx = (state.candidate_idx + 1) % state.candidates.len();
            let next = state.candidates[next_idx].clone();
            state.candidate_idx = next_idx;
            self.location.set(&next);
            frame.textedit_set_caret("##location", next.len() as u32);
            self.state.set_status(format!(
                "[{}/{}] {} (Tab to cycle)",
                next_idx + 1,
                state.candidates.len(),
                next
            ));
            return;
        }

        if let Some(comp) = arca_engine::complete_path(self.state.cwd(), &current) {
            if comp.completed != current {
                self.location.set(&comp.completed);
                frame.textedit_set_caret("##location", comp.completed.len() as u32);
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
                frame.textedit_set_caret("##location", first.len() as u32);
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

    /// Request an XDG activation token for a *file* launch.
    ///
    /// optics-window performs this synchronously on the UI thread and bounds
    /// the wait at roughly one second when the compositor never answers. It is
    /// therefore requested only where it can matter: launching an external
    /// application. Navigating into a directory needs no token, and asking for
    /// one made every double-click on a folder pay the full timeout on
    /// compositors without `xdg_activation`.
    fn file_activation_token(&mut self) -> Option<String> {
        #[cfg(test)]
        {
            self.activation_token_requests += 1;
        }
        // Synchronous: polls a private Wayland queue for up to ~1 s.
        let _span = arca_engine::telemetry::span("ui.activation_token");
        optics_window::window_create_activation_token(None)
    }

    /// Navigate using the configured policy. Returns whether the request was
    /// accepted (asynchronous requests report acceptance, not completion).
    pub(crate) fn navigate_to(&mut self, destination: &str) -> bool {
        if self.async_navigation {
            self.state.begin_navigate(destination)
        } else {
            let before = self.state.cwd().to_string();
            self.state.navigate(destination);
            self.state.cwd() != before
        }
    }

    pub(crate) fn go_back(&mut self) {
        if self.async_navigation {
            self.state.begin_go_back();
        } else {
            self.state.go_back();
        }
    }

    pub(crate) fn go_forward(&mut self) {
        if self.async_navigation {
            self.state.begin_go_forward();
        } else {
            self.state.go_forward();
        }
    }

    pub(crate) fn go_up(&mut self) {
        if self.async_navigation {
            self.state.begin_go_up();
        } else {
            self.state.go_up();
        }
    }

    /// Open a child of the current directory, requesting an activation token
    /// only for non-navigable entries.
    pub(crate) fn open_child(&mut self, name: &str) {
        let navigable = self
            .state
            .entries()
            .iter()
            .find(|entry| entry.name == name)
            .map(|entry| entry.navigable);
        match navigable {
            Some(false) => {
                let token = self.file_activation_token();
                self.state.open_child_with_token(name, token.as_deref());
            }
            Some(true) => {
                let target = arca_engine::entry::child_path(self.state.cwd(), name);
                self.navigate_to(&target);
            }
            // A racing deletion still goes through the engine's token-free
            // open path, which reports the failure in the status bar.
            None => self.state.open_child(name),
        }
    }

    /// Open the current selection with the same token policy as [`Self::open_child`].
    pub(crate) fn open_selected(&mut self) {
        let Some(name) = self.state.selected_entry().map(|entry| entry.name.clone()) else {
            return;
        };
        self.open_child(&name);
    }

    pub(crate) fn row_clicked(&mut self, visible_idx: usize, name: &str) {
        let now = Instant::now();
        let identity = std::path::Path::new(self.state.cwd())
            .join(name)
            .to_string_lossy()
            .into_owned();
        let ctrl = self.mods & optics_ui::mods::CTRL != 0;
        let shift = self.mods & optics_ui::mods::SHIFT != 0;

        if ctrl {
            self.state.toggle_select_visible(visible_idx);
            self.last_click = Some((identity.clone(), now));
            if let Some(chooser) = &mut self.chooser
                && chooser.request.mode == FileChooserMode::OpenFile
                && chooser.allows_multiple()
            {
                let path = std::path::Path::new(self.state.cwd()).join(name);
                chooser.toggle_selected_path(path);
            }
            return;
        }
        if shift {
            self.state.select_range_visible(visible_idx);
            self.last_click = Some((identity.clone(), now));
            return;
        }

        let is_double_click = self.last_click.as_ref().is_some_and(|(last_path, at)| {
            *last_path == identity && now.duration_since(*at).as_millis() < DOUBLE_CLICK_MS
        });
        if is_double_click {
            self.last_click = None;
            self.preview.close();
            if let Some(chooser) = &mut self.chooser {
                let entry = self.state.entries().iter().find(|e| e.name == name);
                if let Some(entry) = entry {
                    if entry.navigable {
                        self.state.open_child(name);
                        return;
                    } else if chooser.request.mode == FileChooserMode::OpenFile {
                        // Double-click accepts the whole staged selection in
                        // multiple mode, not just the clicked row.
                        let path = std::path::Path::new(self.state.cwd()).join(name);
                        let paths = chooser.accept_selection(Some(path));
                        chooser.accept_paths(paths);
                        optics_window::window_close();
                        return;
                    } else if chooser.request.mode == FileChooserMode::SaveFile {
                        chooser.save_name.set(name);
                        crate::chooser::trigger_accept(self);
                        return;
                    }
                }
            }
            // Directories never ask for an activation token; only a real
            // application launch needs one (and may block on it).
            self.open_child(name);
            return;
        }
        self.last_click = Some((identity.clone(), now));
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

    /// Candidate applications for `mime`, resolved once per MIME type.
    fn applications_for_mime(&mut self, mime: &str) -> Vec<arca_xdg::desktop::DesktopEntry> {
        if let Some(cached) = self.open_with_cache.get(mime) {
            return cached.clone();
        }
        let resolved = {
            let _span = arca_engine::telemetry::span("xdg.applications_for_mime");
            arca_xdg::desktop::applications_for_mime(mime)
        };
        self.open_with_cache
            .insert(mime.to_string(), resolved.clone());
        resolved
    }

    pub(crate) fn row_right_clicked(&mut self, visible_idx: usize, anchor: Rect) {
        if !self.state.is_selected(visible_idx) {
            self.state.select_visible(visible_idx);
        }
        let mime = self
            .state
            .selected_entry()
            .filter(|e| !e.navigable)
            .map(|e| e.mime_type.clone());
        let open_with = mime
            .map(|mime| self.applications_for_mime(&mime))
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
        self.last_click = None;
        let target = arca_engine::entry::child_path(directory, name);
        if self.async_navigation && arca_engine::entry::dir_exists(&target) {
            self.navigate_to(&target);
            return;
        }
        // Selecting a file in an ancestor column first changes the active
        // directory; that composite operation stays synchronous.
        self.state.activate_miller_item(directory, name);
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
            || self.renaming.is_some()
            || chooser_save_focused;

        for ev in &raw.keys[..raw.key_count as usize] {
            if !ev.pressed {
                continue;
            }
            let key = ev.key;

            if key == optics_ui::key::ESCAPE {
                // Dismissal order is innermost-first, and every layer that
                // handles Escape consumes the key so optics-ui's transient pass
                // (which runs after the build) cannot dismiss a *different*
                // overlay in the same keystroke.
                if self.renaming.is_some() {
                    frame.consume_key(optics_ui::key::ESCAPE);
                    self.renaming = None;
                    clear_focus(frame);
                    continue;
                }
                if self.location_focused {
                    frame.consume_key(optics_ui::key::ESCAPE);
                    if let Some(comp) = self.location_completion.take() {
                        self.location.set(&comp.original_input);
                        frame.textedit_set_caret("##location", comp.original_input.len() as u32);
                        self.state.set_status(String::new());
                    } else {
                        self.location.set(self.state.cwd());
                        clear_focus(frame);
                        self.location_focused = false;
                        self.location_focused_now = false;
                    }
                    continue;
                }
                if self.filter_focused {
                    frame.consume_key(optics_ui::key::ESCAPE);
                    clear_focus(frame);
                    self.filter_focused = false;
                    self.filter_focused_now = false;
                    continue;
                }
                // Quick Look is a modal layer above the listing: Escape
                // closes it before anything else can react.
                if self.preview.target_open {
                    frame.consume_key(optics_ui::key::ESCAPE);
                    self.preview.close();
                    continue;
                }
                // A long transfer is cancellable; cancellation outranks any
                // remaining dismissal layer because it is the only way to
                // stop disk work the user started.
                if self.state.is_transferring() {
                    frame.consume_key(optics_ui::key::ESCAPE);
                    self.state.cancel_transfer();
                    continue;
                }
                if let Some(chooser) = &mut self.chooser {
                    frame.consume_key(optics_ui::key::ESCAPE);
                    if chooser.overwrite_confirm.is_some() {
                        chooser.overwrite_confirm = None;
                    } else if chooser.filter_menu_open {
                        chooser.filter_menu_open = false;
                    } else {
                        chooser.cancel();
                        optics_window::window_close();
                    }
                    continue;
                }
            }

            if text_editing {
                if key == optics_ui::key::RETURN {
                    frame.consume_key(optics_ui::key::RETURN);
                    if self.rename_focused {
                        self.commit_rename(frame);
                    } else if chooser_save_focused {
                        chooser::trigger_accept(self);
                    }
                } else if key == optics_ui::key::TAB && self.location_focused {
                    frame.consume_key(optics_ui::key::TAB);
                }
                continue;
            }

            let ctrl = mods & optics_ui::mods::CTRL != 0;
            let alt = mods & optics_ui::mods::ALT != 0;
            let shift = mods & optics_ui::mods::SHIFT != 0;

            const KEY_SPACE: i32 = b' ' as i32;

            match key {
                optics_ui::key::TAB if ctrl => {
                    frame.consume_key(optics_ui::key::TAB);
                    self.state.cycle_tab(shift);
                    self.sync_active_tab();
                }
                optics_ui::key::LEFT if alt => self.go_back(),
                optics_ui::key::RIGHT if alt => self.go_forward(),
                optics_ui::key::UP if alt => self.go_up(),
                optics_ui::key::BACKSPACE if !ctrl && !alt => self.go_up(),
                optics_ui::key::LEFT if !ctrl && !alt && self.state.view_mode == ViewMode::Grid => {
                    self.state.move_selection(-1)
                }
                optics_ui::key::RIGHT
                    if !ctrl && !alt && self.state.view_mode == ViewMode::Grid =>
                {
                    self.state.move_selection(1)
                }
                optics_ui::key::LEFT
                    if !ctrl && !alt && self.state.view_mode == ViewMode::Miller =>
                {
                    self.go_up()
                }
                optics_ui::key::RIGHT
                    if !ctrl && !alt && self.state.view_mode == ViewMode::Miller =>
                {
                    self.preview.close();
                    self.open_selected();
                }
                optics_ui::key::UP if !ctrl && !alt => {
                    let step = if self.state.view_mode == ViewMode::Grid {
                        self.grid_columns() as i64
                    } else {
                        1
                    };
                    self.state.move_selection_ext(-step, shift);
                }
                optics_ui::key::DOWN if !ctrl && !alt => {
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
                optics_ui::key::RETURN if !ctrl && !alt => {
                    if self.chooser.is_some() {
                        chooser::trigger_accept(self);
                    } else {
                        self.preview.close();
                        self.open_selected();
                    }
                }
                optics_ui::key::DELETE if shift && !ctrl && !alt => {
                    self.preview.close();
                    self.state.delete_selected_permanently();
                }
                optics_ui::key::DELETE if !ctrl && !alt => {
                    self.preview.close();
                    self.state.trash_selected();
                }
                optics_ui::key::F2 if !ctrl && !alt => {
                    if let Some(name) = self.state.selected_entry().map(|entry| entry.name.clone())
                    {
                        self.begin_rename(name);
                    }
                }
                optics_ui::key::F3 if !ctrl && !alt => self.focus_filter(),
                optics_ui::key::F5 if !alt => self.state.refresh(),
                optics_ui::key::F11 if !ctrl && !alt => {
                    self.fullscreen = !self.fullscreen;
                    if self.fullscreen {
                        optics_window::window_fullscreen();
                    } else {
                        optics_window::window_windowed();
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

    use crate::test_support::{fixture, fixture_two_files, frame, press, settle};
    use arca_engine::TransferAction;

    #[test]
    fn builds_frames_headless() {
        let (_dir, mut app) = fixture();
        let mut ui = optics_ui::Ui::headless().expect("headless ui");
        let input = optics_ui::Input::new((1040.0, 700.0), 1.0 / 60.0);
        // Several frames so the retained tree reconciles enter/stable/leave.
        for _ in 0..3 {
            frame(&mut app, &mut ui, &input);
        }
    }

    #[test]
    fn ctrl_h_toggles_hidden_files() {
        let (_dir, mut app) = fixture();
        let mut ui = optics_ui::Ui::headless().expect("headless ui");
        assert!(!app.state.show_hidden);
        assert_eq!(app.state.entries().len(), 2);

        let mut input = optics_ui::Input::new((1040.0, 700.0), 1.0 / 60.0);
        input.set_mods(optics_ui::mods::CTRL);
        input.push_key('h' as i32, true, false);
        frame(&mut app, &mut ui, &input);

        assert!(app.state.show_hidden);
        assert_eq!(app.state.entries().len(), 3);
    }

    #[test]
    fn arrows_select_and_return_enters_directory() {
        let (_dir, mut app) = fixture();
        let mut ui = optics_ui::Ui::headless().expect("headless ui");

        let mut down = optics_ui::Input::new((1040.0, 700.0), 1.0 / 60.0);
        down.push_key(optics_ui::key::DOWN, true, false);
        frame(&mut app, &mut ui, &down);
        assert_eq!(
            app.state.selected_entry().map(|e| e.name.as_str()),
            Some("sub"),
            "first entry is the sub directory (dirs sort first)"
        );

        let mut enter = optics_ui::Input::new((1040.0, 700.0), 1.0 / 60.0);
        enter.push_key(optics_ui::key::RETURN, true, false);
        frame(&mut app, &mut ui, &enter);
        assert!(app.state.cwd().ends_with("/sub"));
    }

    #[test]
    fn filter_field_filters_the_list() {
        let (_dir, mut app) = fixture();
        let mut ui = optics_ui::Ui::headless().expect("headless ui");
        app.state.set_filter("txt".into());
        let input = optics_ui::Input::new((1040.0, 700.0), 1.0 / 60.0);
        frame(&mut app, &mut ui, &input);
        assert_eq!(app.state.visible().len(), 1);
    }

    #[test]
    fn double_click_into_directory_does_not_panic_on_stale_rows() {
        let (_dir, mut app) = fixture();
        // List rows record `sub_row_rect`; Grid is the default view.
        app.state.override_view_mode(arca_engine::ViewMode::List);
        let mut ui = optics_ui::Ui::headless().expect("headless ui");
        let settle = optics_ui::Input::new((1040.0, 700.0), 1.0 / 60.0);
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
        assert_eq!(app.activation_token_requests, 0);
        for _ in 0..2 {
            let mut press = optics_ui::Input::new((1040.0, 700.0), 1.0 / 60.0);
            press.set_cursor(row0.0, row0.1);
            press.set_mouse_down(optics_ui::MouseButton::Left, true);
            press.set_mouse_pressed(optics_ui::MouseButton::Left, true);
            frame(&mut app, &mut ui, &press);
            let mut release = optics_ui::Input::new((1040.0, 700.0), 1.0 / 60.0);
            release.set_cursor(row0.0, row0.1);
            release.set_mouse_released(optics_ui::MouseButton::Left, true);
            frame(&mut app, &mut ui, &release);
        }
        assert!(app.state.cwd().ends_with("/sub"));
        assert_eq!(
            app.activation_token_requests, 0,
            "navigating into a directory must not request a synchronous activation token"
        );
        for _ in 0..3 {
            frame(&mut app, &mut ui, &settle);
        }
    }

    /// Double-click detection follows the entry's absolute path, not its
    /// visible index or bare name. Clicking row 0, navigating elsewhere, then
    /// clicking row 0 again — even on a same-named entry — used to look like a
    /// double-click on the *new* entry and launch it.
    #[test]
    fn double_click_does_not_cross_a_navigation_to_a_different_entry() {
        let (dir, mut app) = fixture();
        let outer = dir.join("outer");
        std::fs::create_dir_all(outer.join("sub")).unwrap();

        // First click selects the directory at visible index 0.
        app.row_clicked(0, "sub");
        assert_eq!(
            app.state.selected_entry().map(|entry| entry.name.as_str()),
            Some("sub")
        );

        // Navigation rebuilds the visible index space. A click at the same
        // index, even on a same-named entry, is a different absolute path and
        // must not form a double-click.
        app.state.navigate(outer.to_str().unwrap());
        app.row_clicked(0, "sub");

        assert_eq!(
            app.state.cwd(),
            outer.to_str().unwrap(),
            "a same-named entry in another directory must not replay the previous double-click"
        );
        assert_eq!(
            app.state.selected_entry().map(|entry| entry.name.as_str()),
            Some("sub"),
            "the click selects the new row"
        );
    }

    #[test]
    fn keyboard_shortcuts_rename_and_delete_undo() {
        let (_dir, mut app) = fixture();
        let mut ui = optics_ui::Ui::headless().expect("headless ui");
        let settle = optics_ui::Input::new((1040.0, 700.0), 1.0 / 60.0);
        frame(&mut app, &mut ui, &settle);

        // Select first item
        let mut down = optics_ui::Input::new((1040.0, 700.0), 1.0 / 60.0);
        down.push_key(optics_ui::key::DOWN, true, false);
        frame(&mut app, &mut ui, &down);
        assert!(app.state.selected().is_some());

        // Press Ctrl+E to begin rename
        let mut rename_in = optics_ui::Input::new((1040.0, 700.0), 1.0 / 60.0);
        rename_in.set_mods(optics_ui::mods::CTRL);
        rename_in.push_key('e' as i32, true, false);
        frame(&mut app, &mut ui, &rename_in);
        assert!(app.renaming.is_some());

        // Cancel rename with Esc
        let mut esc = optics_ui::Input::new((1040.0, 700.0), 1.0 / 60.0);
        esc.push_key(optics_ui::key::ESCAPE, true, false);
        frame(&mut app, &mut ui, &esc);
        assert!(app.renaming.is_none());

        // Delete item to trash
        let mut del = optics_ui::Input::new((1040.0, 700.0), 1.0 / 60.0);
        del.push_key(optics_ui::key::DELETE, true, false);
        frame(&mut app, &mut ui, &del);
        assert_eq!(app.state.entries().len(), 1);

        // Ctrl+Z to undo trash
        let mut undo = optics_ui::Input::new((1040.0, 700.0), 1.0 / 60.0);
        undo.set_mods(optics_ui::mods::CTRL);
        undo.push_key('z' as i32, true, false);
        frame(&mut app, &mut ui, &undo);
        assert_eq!(app.state.entries().len(), 2);
    }

    #[test]
    fn keyboard_shortcuts_function_keys() {
        let (_dir, mut app) = fixture();
        let mut ui = optics_ui::Ui::headless().expect("headless ui");
        let settle = optics_ui::Input::new((1040.0, 700.0), 1.0 / 60.0);
        frame(&mut app, &mut ui, &settle);

        // Select first item
        let mut down = optics_ui::Input::new((1040.0, 700.0), 1.0 / 60.0);
        down.push_key(optics_ui::key::DOWN, true, false);
        frame(&mut app, &mut ui, &down);
        assert!(app.state.selected().is_some());

        // Press F2 to begin rename
        let mut f2 = optics_ui::Input::new((1040.0, 700.0), 1.0 / 60.0);
        f2.push_key(optics_ui::key::F2, true, false);
        frame(&mut app, &mut ui, &f2);
        assert!(app.renaming.is_some());

        // Cancel rename with Esc
        let mut esc = optics_ui::Input::new((1040.0, 700.0), 1.0 / 60.0);
        esc.push_key(optics_ui::key::ESCAPE, true, false);
        frame(&mut app, &mut ui, &esc);
        assert!(app.renaming.is_none());

        // Press F3 to focus search/filter
        let mut f3 = optics_ui::Input::new((1040.0, 700.0), 1.0 / 60.0);
        f3.push_key(optics_ui::key::F3, true, false);
        frame(&mut app, &mut ui, &f3);
        frame(&mut app, &mut ui, &settle);
        assert!(app.filter_focused);

        // Escape clears filter focus
        let mut esc_filter = optics_ui::Input::new((1040.0, 700.0), 1.0 / 60.0);
        esc_filter.push_key(optics_ui::key::ESCAPE, true, false);
        frame(&mut app, &mut ui, &esc_filter);
        frame(&mut app, &mut ui, &settle);
        assert!(!app.filter_focused);

        // Press F5 to refresh
        let mut f5 = optics_ui::Input::new((1040.0, 700.0), 1.0 / 60.0);
        f5.push_key(optics_ui::key::F5, true, false);
        frame(&mut app, &mut ui, &f5);

        // Press F11 to toggle fullscreen
        assert!(!app.fullscreen);
        let mut f11 = optics_ui::Input::new((1040.0, 700.0), 1.0 / 60.0);
        f11.push_key(optics_ui::key::F11, true, false);
        frame(&mut app, &mut ui, &f11);
        assert!(app.fullscreen);

        let mut f11_off = optics_ui::Input::new((1040.0, 700.0), 1.0 / 60.0);
        f11_off.push_key(optics_ui::key::F11, true, false);
        frame(&mut app, &mut ui, &f11_off);
        assert!(!app.fullscreen);
    }

    #[test]
    fn paste_from_external_system_clipboard_uri_list() {
        let (dir, mut app) = fixture();
        let mut ui = optics_ui::Ui::headless().expect("headless ui");
        let settle = optics_ui::Input::new((1040.0, 700.0), 1.0 / 60.0);
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

        let mut paste_input = optics_ui::Input::new((1040.0, 700.0), 1.0 / 60.0);
        paste_input.set_mods(optics_ui::mods::CTRL);
        paste_input.push_key('v' as i32, true, false);
        frame(&mut app, &mut ui, &paste_input);
        frame(&mut app, &mut ui, &settle);

        assert!(sub_dir.join("a.txt").exists());
        assert_eq!(app.state.entries().len(), 1);
    }

    #[test]
    fn ui_multi_selection_ctrl_a_and_batch_trash() {
        let (_dir, mut app) = fixture();
        let mut ui = optics_ui::Ui::headless().expect("headless ui");
        let settle = optics_ui::Input::new((1040.0, 700.0), 1.0 / 60.0);
        frame(&mut app, &mut ui, &settle);

        // Select all with Ctrl+A
        let mut ctrl_a = optics_ui::Input::new((1040.0, 700.0), 1.0 / 60.0);
        ctrl_a.set_mods(optics_ui::mods::CTRL);
        ctrl_a.push_key('a' as i32, true, false);
        frame(&mut app, &mut ui, &ctrl_a);

        assert_eq!(app.state.selected_count(), 2);
        assert_eq!(app.state.selected_paths().len(), 2);

        // Batch delete selected items to trash
        let mut del = optics_ui::Input::new((1040.0, 700.0), 1.0 / 60.0);
        del.push_key(optics_ui::key::DELETE, true, false);
        frame(&mut app, &mut ui, &del);

        assert_eq!(app.state.entries().len(), 0);

        // Ctrl+Z restores all items
        let mut undo = optics_ui::Input::new((1040.0, 700.0), 1.0 / 60.0);
        undo.set_mods(optics_ui::mods::CTRL);
        undo.push_key('z' as i32, true, false);
        frame(&mut app, &mut ui, &undo);

        assert_eq!(app.state.entries().len(), 2);
    }

    #[test]
    fn ctrl_t_opens_tab_and_space_preview_animates() {
        let (_dir, mut app) = fixture();
        let mut ui = optics_ui::Ui::headless().expect("headless ui");

        let mut new_tab = optics_ui::Input::new((1040.0, 700.0), 1.0 / 60.0);
        new_tab.set_mods(optics_ui::mods::CTRL);
        new_tab.push_key('t' as i32, true, false);
        frame(&mut app, &mut ui, &new_tab);
        assert_eq!(app.state.tabs().len(), 2);

        app.state.move_selection(1);
        let mut space = optics_ui::Input::new((1040.0, 700.0), 1.0 / 60.0);
        space.push_key(' ' as i32, true, false);
        frame(&mut app, &mut ui, &space);
        assert!(app.preview.target_open);

        let settle = optics_ui::Input::new((1040.0, 700.0), 1.0 / 60.0);
        for _ in 0..8 {
            frame(&mut app, &mut ui, &settle);
        }
        assert!(app.preview.progress > 0.5);
    }

    /// Focus targets are stored as stable widget labels, not numeric ids
    /// captured from an earlier build (Optics v0.0.76 host focus contract).
    /// The label must resolve after the rebuild that creates the widget and
    /// release cleanly when the field is dismissed.
    #[test]
    fn focus_targets_resolve_by_label_across_rebuilds() {
        let (_dir, mut app) = fixture();
        let mut ui = optics_ui::Ui::headless().expect("headless ui");
        let settle = optics_ui::Input::new((1040.0, 700.0), 1.0 / 60.0);
        for _ in 0..4 {
            frame(&mut app, &mut ui, &settle);
        }

        // Ctrl+L focuses the location field.
        press(&mut app, &mut ui, 'l' as i32, optics_ui::mods::CTRL);
        frame(&mut app, &mut ui, &settle);
        assert!(app.location_focused, "location must take focus");
        assert!(!app.filter_focused);
        assert_eq!(app.pending_focus, None, "the label must resolve in-frame");

        // Escape releases it; no stale id may pull focus back on the next build.
        press(&mut app, &mut ui, optics_ui::key::ESCAPE, 0);
        frame(&mut app, &mut ui, &settle);
        assert!(
            !app.location_focused,
            "dismissed location field must not retain focus"
        );

        // F3 focuses the filter field by its own stable label.
        press(&mut app, &mut ui, optics_ui::key::F3, 0);
        frame(&mut app, &mut ui, &settle);
        assert!(app.filter_focused, "filter must take focus");
        press(&mut app, &mut ui, optics_ui::key::ESCAPE, 0);
        frame(&mut app, &mut ui, &settle);
        assert!(!app.filter_focused);

        // F2 focuses the inline rename field of the selected List row.
        app.state.select_visible(1);
        press(&mut app, &mut ui, optics_ui::key::F2, 0);
        frame(&mut app, &mut ui, &settle);
        assert!(
            app.rename_focused,
            "inline rename must take focus through its view-specific label"
        );
        assert_eq!(app.pending_focus, None, "the rename label must resolve");
    }

    #[test]
    fn location_bar_tab_completion_works() {
        let (dir, mut app) = fixture();
        let mut ui = optics_ui::Ui::headless().expect("headless ui");
        let input = optics_ui::Input::new((1040.0, 700.0), 1.0 / 60.0);
        for _ in 0..2 {
            frame(&mut app, &mut ui, &input);
        }

        app.focus_location();
        for _ in 0..2 {
            frame(&mut app, &mut ui, &input);
        }

        let partial = format!("{}/s", dir.to_str().unwrap());
        app.location.set(&partial);

        let mut tab = optics_ui::Input::new((1040.0, 700.0), 1.0 / 60.0);
        tab.push_key(optics_ui::key::TAB, true, false);
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
        let mut ui = optics_ui::Ui::headless().expect("headless ui");
        let input = optics_ui::Input::new((1040.0, 700.0), 1.0 / 60.0);
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
        let mut ui = optics_ui::Ui::headless().expect("headless ui");
        app.start(&mut ui);
        assert!(app.icons.is_registered_on(&mut ui));
        app.shutdown(&mut ui);
        assert!(crate::device::device().is_none());
        assert_eq!(
            app.icons,
            crate::icons::IconSet::EMPTY,
            "shutdown releases the context's icons"
        );
    }

    #[test]
    fn filter_textfield_hover_cursor_hint() {
        let mut ui = optics_ui::Ui::headless().expect("headless ui");
        let mut tf_buf = optics_ui::TextBuf::new(64, "test");
        let input = optics_ui::Input::new((400.0, 200.0), 1.0 / 60.0);

        // Frame 1: resolve layout so node has prev_rect
        ui.frame(&input, |f| {
            f.size_next(200.0, 36.0);
            f.textedit(
                "##tf",
                &mut tf_buf,
                &optics_ui::TextEditOpts {
                    placeholder: Some("placeholder"),
                    ..Default::default()
                },
            );
        });
        if let Ok(snap) = ui.snapshot() {
            let _ = ui.activate(&snap);
        }

        // Frame 2: hover cursor over the textfield at (50, 18)
        let mut hover_input = optics_ui::Input::new((400.0, 200.0), 1.0 / 60.0);
        hover_input.set_cursor(50.0, 18.0);
        ui.frame(&hover_input, |f| {
            f.size_next(200.0, 36.0);
            f.textedit(
                "##tf",
                &mut tf_buf,
                &optics_ui::TextEditOpts {
                    placeholder: Some("placeholder"),
                    ..Default::default()
                },
            );
        });

        assert_eq!(ui.cursor_hint(), optics_ui::CursorHint::Text);
    }

    #[test]
    fn every_build_beats_the_watchdog_heartbeat_and_marks_the_ui_thread() {
        let (_dir, mut app) = fixture();
        let mut ui = optics_ui::Ui::headless().expect("headless ui");
        let heartbeat = arca_engine::telemetry::watchdog::heartbeat();
        let before = heartbeat.frames();
        settle(&mut app, &mut ui);
        assert!(heartbeat.frames() > before, "frames were counted");
        assert!(arca_engine::telemetry::span::is_ui_thread());
        assert!(
            arca_engine::telemetry::metrics::global()
                .histogram("ui.frame")
                .is_some_and(|h| h.count() > 0)
        );
    }

    #[test]
    fn alerts_reach_the_status_bar_only_when_the_window_opts_in() {
        use arca_engine::telemetry::alerts::{self, Alert, AlertKind, Severity};
        let mut ui = optics_ui::Ui::headless().expect("headless ui");

        let (_dir, quiet) = fixture();
        let (_dir2, loud) = fixture();
        let mut loud = loud.surface_alerts();
        let mut quiet = quiet;
        // Unique summary: the global log deduplicates by summary stem.
        let summary = "UI unresponsive (status-bar routing test)";
        alerts::global().raise(Alert::new(AlertKind::UiStall, Severity::Critical, summary));

        settle(&mut quiet, &mut ui);
        assert!(
            !quiet.state.status.as_deref().unwrap_or("").starts_with('⚠'),
            "opted-out windows never show alerts"
        );
        // The notice is still pending (the quiet window did not consume it).
        // Sibling tests in this process may raise a newer warning meanwhile,
        // so assert the routing, not the exact text.
        settle(&mut loud, &mut ui);
        assert!(
            loud.state
                .status
                .as_deref()
                .is_some_and(|s| s.starts_with('⚠')),
            "status was {:?}",
            loud.state.status
        );
    }

    /// Deleting the last remaining tab is documented as exiting the app; the
    /// engine refuses, so the UI must close the window explicitly rather than
    /// silently doing nothing.
    #[test]
    fn ctrl_w_on_the_last_tab_requests_window_close() {
        let (_dir, mut app) = fixture();
        let mut ui = optics_ui::Ui::headless().expect("headless ui");
        let settle = optics_ui::Input::new((1040.0, 700.0), 1.0 / 60.0);
        frame(&mut app, &mut ui, &settle);
        assert_eq!(app.state.tabs().len(), 1);

        press(&mut app, &mut ui, 'w' as i32, optics_ui::mods::CTRL);

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
        let mut ui = optics_ui::Ui::headless().expect("headless ui");
        let settle = optics_ui::Input::new((1040.0, 700.0), 1.0 / 60.0);
        frame(&mut app, &mut ui, &settle);
        assert!(!app.close_requested);

        press(&mut app, &mut ui, 'q' as i32, optics_ui::mods::CTRL);
        assert!(app.close_requested, "Ctrl+Q must request a window close");
    }

    /// A re-read of the directory (Ctrl+R, or any inotify event) used to
    /// leave the cursor index pointing at a row that was no longer marked
    /// selected — the highlight vanished.
    #[test]
    fn refresh_keeps_the_selection_highlight() {
        let (_dir, mut app) = fixture();
        let mut ui = optics_ui::Ui::headless().expect("headless ui");
        let settle = optics_ui::Input::new((1040.0, 700.0), 1.0 / 60.0);
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
        let mut ui = optics_ui::Ui::headless().expect("headless ui");
        let settle = optics_ui::Input::new((1040.0, 700.0), 1.0 / 60.0);
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
        let mut ui = optics_ui::Ui::headless().expect("headless ui");
        let settle = optics_ui::Input::new((1040.0, 700.0), 1.0 / 60.0);
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
        let mut ui = optics_ui::Ui::headless().expect("headless ui");
        let settle = optics_ui::Input::new((1040.0, 700.0), 1.0 / 60.0);
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
        let mut ui = optics_ui::Ui::headless().expect("headless ui");
        let settle = optics_ui::Input::new((1040.0, 700.0), 1.0 / 60.0);
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
        let mut ui = optics_ui::Ui::headless().expect("headless ui");
        let input = optics_ui::Input::new((800.0, 600.0), 1.0 / 60.0);

        let mut cols = vec![
            optics_ui::patterns::DataGridCol::new("Name").width(200.0),
            optics_ui::patterns::DataGridCol::new("Size").width(100.0),
            optics_ui::patterns::DataGridCol::new("Modified").width(150.0),
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
            optics_ui::patterns::TreeItem::new("Root Folder").expanded(true),
            optics_ui::patterns::TreeItem::new("Documents")
                .depth(1)
                .expanded(false),
            optics_ui::patterns::TreeItem::new("file.txt")
                .depth(2)
                .leaf(true),
        ];

        ui.frame(&input, |f| {
            // DataGrid pattern
            let grid_act = optics_ui::patterns::DataGrid::new("test_files_grid")
                .striped(true)
                .show(f, &mut cols, &cells, 2, &mut selected_row);
            assert_eq!(grid_act, optics_ui::patterns::DataGridAction::None);

            // TreeView pattern
            let tree_act =
                optics_ui::patterns::TreeView::new("test_files_tree").show(f, &mut tree_items);
            assert_eq!(tree_act, optics_ui::patterns::TreeAction::None);
        });

        assert!(!ui.overflowed());
    }
}
