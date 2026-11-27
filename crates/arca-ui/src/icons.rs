//! Icon glyphs for files, folders and chrome.
//!
//! Arca owns its icon artwork as SVG in `assets/icons/*.svg` (see the README
//! there). Optics carries no format parsers (Optics ADR-0174), so
//! [`vector`] decodes each asset into format-agnostic vector commands and
//! [`IconSet::register`] registers the whole set on one `optics_ui` context
//! (ADR-0025). Icon ids are generational handles owned by that context: an
//! [`IconSet`] is valid only for the [`Ui`] it was registered on, and is
//! released with [`IconSet::unregister`] before the context goes away. Glyphs
//! are drawn through the native icon widgets: vector-crisp at any size,
//! theme-tinted where the artwork uses `currentColor`, no raster assets.

mod vector;

use arca_engine::bookmarks::BookmarkKind;
use arca_engine::entry::{Entry, FileType};
use optics_ui::{Frame, Ui};
use optics_ui_sys::optics_ui_icon_id;

/// One glyph of arca's icon set; the variant name is the PascalCase of
/// the SVG file name in `assets/icons/`.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub(crate) enum AssetId {
    AlertCircle,
    Aperture,
    Archive,
    ArrowLeft,
    ArrowRight,
    ArrowUp,
    ChevronDown,
    ChevronRight,
    ChevronUp,
    Code,
    Columns,
    Download,
    Eye,
    EyeOff,
    File,
    FileText,
    Film,
    Folder,
    GitBranch,
    Grid,
    Home,
    Image,
    Link2,
    List,
    Monitor,
    Moon,
    Music,
    Plus,
    RefreshCw,
    Search,
    Settings,
    Sun,
    Terminal,
    Trash,
    X,
}

pub(crate) mod ids {
    //! Keeps call sites reading `ids::X`.
    pub(crate) use super::AssetId::*;
}

impl AssetId {
    /// Number of glyphs in the set.
    pub(crate) const COUNT: usize = AssetId::X as usize + 1;

    pub(crate) const ALL: [AssetId; Self::COUNT] = [
        AssetId::AlertCircle,
        AssetId::Aperture,
        AssetId::Archive,
        AssetId::ArrowLeft,
        AssetId::ArrowRight,
        AssetId::ArrowUp,
        AssetId::ChevronDown,
        AssetId::ChevronRight,
        AssetId::ChevronUp,
        AssetId::Code,
        AssetId::Columns,
        AssetId::Download,
        AssetId::Eye,
        AssetId::EyeOff,
        AssetId::File,
        AssetId::FileText,
        AssetId::Film,
        AssetId::Folder,
        AssetId::GitBranch,
        AssetId::Grid,
        AssetId::Home,
        AssetId::Image,
        AssetId::Link2,
        AssetId::List,
        AssetId::Monitor,
        AssetId::Moon,
        AssetId::Music,
        AssetId::Plus,
        AssetId::RefreshCw,
        AssetId::Search,
        AssetId::Settings,
        AssetId::Sun,
        AssetId::Terminal,
        AssetId::Trash,
        AssetId::X,
    ];

    /// The SVG source, embedded at compile time.
    fn svg(self) -> &'static str {
        match self {
            AssetId::AlertCircle => include_str!("../../../assets/icons/alert-circle.svg"),
            AssetId::Aperture => include_str!("../../../assets/icons/aperture.svg"),
            AssetId::Archive => include_str!("../../../assets/icons/archive.svg"),
            AssetId::ArrowLeft => include_str!("../../../assets/icons/arrow-left.svg"),
            AssetId::ArrowRight => include_str!("../../../assets/icons/arrow-right.svg"),
            AssetId::ArrowUp => include_str!("../../../assets/icons/arrow-up.svg"),
            AssetId::ChevronDown => include_str!("../../../assets/icons/chevron-down.svg"),
            AssetId::ChevronRight => include_str!("../../../assets/icons/chevron-right.svg"),
            AssetId::ChevronUp => include_str!("../../../assets/icons/chevron-up.svg"),
            AssetId::Code => include_str!("../../../assets/icons/code.svg"),
            AssetId::Columns => include_str!("../../../assets/icons/columns.svg"),
            AssetId::Download => include_str!("../../../assets/icons/download.svg"),
            AssetId::Eye => include_str!("../../../assets/icons/eye.svg"),
            AssetId::EyeOff => include_str!("../../../assets/icons/eye-off.svg"),
            AssetId::File => include_str!("../../../assets/icons/file.svg"),
            AssetId::FileText => include_str!("../../../assets/icons/file-text.svg"),
            AssetId::Film => include_str!("../../../assets/icons/film.svg"),
            AssetId::Folder => include_str!("../../../assets/icons/folder.svg"),
            AssetId::GitBranch => include_str!("../../../assets/icons/git-branch.svg"),
            AssetId::Grid => include_str!("../../../assets/icons/grid.svg"),
            AssetId::Home => include_str!("../../../assets/icons/home.svg"),
            AssetId::Image => include_str!("../../../assets/icons/image.svg"),
            AssetId::Link2 => include_str!("../../../assets/icons/link-2.svg"),
            AssetId::List => include_str!("../../../assets/icons/list.svg"),
            AssetId::Monitor => include_str!("../../../assets/icons/monitor.svg"),
            AssetId::Moon => include_str!("../../../assets/icons/moon.svg"),
            AssetId::Music => include_str!("../../../assets/icons/music.svg"),
            AssetId::Plus => include_str!("../../../assets/icons/plus.svg"),
            AssetId::RefreshCw => include_str!("../../../assets/icons/refresh-cw.svg"),
            AssetId::Search => include_str!("../../../assets/icons/search.svg"),
            AssetId::Settings => include_str!("../../../assets/icons/settings.svg"),
            AssetId::Sun => include_str!("../../../assets/icons/sun.svg"),
            AssetId::Terminal => include_str!("../../../assets/icons/terminal.svg"),
            AssetId::Trash => include_str!("../../../assets/icons/trash.svg"),
            AssetId::X => include_str!("../../../assets/icons/x.svg"),
        }
    }
}

/// Arca's glyphs registered on one `optics_ui` context.
///
/// A plain table of generational ids, `Copy` so builders can hold it across
/// closures that also borrow the app mutably. [`IconSet::EMPTY`] maps every
/// glyph to `OPTICS_UI_ICON_NONE`, for which widgets draw no glyph.
///
/// Generational ids do not name their context: two fresh contexts hand out
/// the same first ids. The set therefore records the address of the context
/// it was registered on, so releasing it on a different context (which would
/// free that context's same-numbered glyphs) is caught instead of silent.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct IconSet {
    ids: [optics_ui_icon_id; AssetId::COUNT],
    /// Address of the owning `optics_ui`, or 0 for [`IconSet::EMPTY`]. Kept
    /// as an address, never dereferenced.
    owner: usize,
}

impl Default for IconSet {
    fn default() -> Self {
        Self::EMPTY
    }
}

impl IconSet {
    pub(crate) const EMPTY: IconSet = IconSet {
        ids: [optics_ui_icon_id::OPTICS_UI_ICON_NONE; AssetId::COUNT],
        owner: 0,
    };

    /// Decode every asset and register it on `ui`. A glyph that fails to
    /// decode or register is logged and left as `OPTICS_UI_ICON_NONE`; the
    /// `every_asset_registers` test keeps that from shipping.
    pub(crate) fn register(ui: &mut Ui) -> IconSet {
        let mut set = IconSet {
            owner: ui.as_raw() as usize,
            ..IconSet::EMPTY
        };
        for asset in AssetId::ALL {
            let registered = vector::decode(asset.svg())
                .map_err(|e| e.to_string())
                .and_then(|icon| {
                    ui.register_icon(
                        &icon.cmds,
                        &icon.runs,
                        optics_ui_sys::optics_ui_icon_paint::OPTICS_UI_ICON_PAINT_FILL,
                    )
                    .ok_or_else(|| "optics-ui rejected the command stream".to_owned())
                });
            match registered {
                Ok(id) => set.ids[asset as usize] = id,
                Err(e) => log::warn!(target: "arca_ui", "icon {asset:?} unavailable: {e}"),
            }
        }
        set
    }

    /// Release every glyph from `ui` and reset to [`IconSet::EMPTY`].
    ///
    /// # Panics
    ///
    /// If `ui` is not the context the set was registered on. An empty set
    /// may be released on any context.
    pub(crate) fn unregister(&mut self, ui: &mut Ui) {
        if self.owner == 0 {
            return;
        }
        assert_eq!(
            self.owner,
            ui.as_raw() as usize,
            "IconSet released on a context it was not registered on"
        );
        for id in self.ids {
            ui.unregister_icon(id);
        }
        *self = IconSet::EMPTY;
    }

    /// Whether every glyph is live on `ui` itself. The address check rules
    /// out ids from another context. The registry lookup rules out a new
    /// context allocated at a dropped one's address.
    #[cfg(test)]
    pub(crate) fn is_registered_on(&self, ui: &mut Ui) -> bool {
        let raw = ui.as_raw();
        self.owner == raw as usize
            && self.ids.iter().all(|&id| {
                // SAFETY: `raw` is live for this borrow; icon_info only reads
                // the context's registry.
                !unsafe { optics_ui_sys::optics_ui_icon_info(raw, id) }.is_null()
            })
    }

    /// The context-local icon id for `asset`.
    pub(crate) fn id(&self, asset: AssetId) -> optics_ui_icon_id {
        self.ids[asset as usize]
    }

    pub(crate) fn icon(&self, frame: &mut Frame, asset: AssetId, size: f32) {
        frame.icon(self.id(asset), size);
    }

    /// Ghost icon button in a square `box_size` hit target.
    /// Borderless and transparent at rest, subtle background fill on hover/press.
    pub(crate) fn button(&self, frame: &mut Frame, asset: AssetId, box_size: f32) -> bool {
        subtle_icon_button(
            frame,
            &format!("##icon-{asset:?}"),
            self.id(asset),
            box_size,
            false,
        )
    }

    /// Icon button that swaps glyph with its checked state (eye/eye-off, star…).
    /// Borderless and transparent at rest; reveals subtle background on hover,
    /// and accent/active surface when checked.
    pub(crate) fn toggle_button(
        &self,
        frame: &mut Frame,
        unchecked: AssetId,
        checked: AssetId,
        box_size: f32,
        is_checked: bool,
    ) -> bool {
        let glyph = if is_checked { checked } else { unchecked };
        subtle_icon_button(
            frame,
            &format!("##icon-toggle-{unchecked:?}-{checked:?}"),
            self.id(glyph),
            box_size,
            is_checked,
        )
    }

    /// Full-width sidebar/menu row with a leading icon, compact 30px height, and 18px icon size.
    pub(crate) fn selectable(
        &self,
        frame: &mut Frame,
        asset: AssetId,
        label: &str,
        selected: bool,
    ) -> bool {
        self.selectable_sized(frame, asset, label, selected, 30.0, 18.0)
    }

    /// Full-width sidebar/menu row with explicit height and icon size.
    pub(crate) fn selectable_sized(
        &self,
        frame: &mut Frame,
        asset: AssetId,
        label: &str,
        selected: bool,
        height: f32,
        icon_size: f32,
    ) -> bool {
        frame.size_next(0.0, height);
        frame.selectable_with_icon(label, self.id(asset), selected, icon_size)
    }
}

/// A subtle-variant icon button with an explicit widget id and square hit
/// target. The safe `Frame::icon_button` takes neither, so this drops to
/// the `optics_ui_button` descriptor.
fn subtle_icon_button(
    frame: &mut Frame,
    id: &str,
    icon: optics_ui_icon_id,
    box_size: f32,
    active: bool,
) -> bool {
    let c = std::ffi::CString::new(id).expect("icon widget ids contain no NUL");
    let opts = optics_ui_sys::optics_ui_button_opts {
        box_: optics_ui_sys::optics_ui_box {
            id: c.as_ptr(),
            width: box_size,
            height: box_size,
            ..Default::default()
        },
        icon,
        variant: optics_ui_sys::optics_ui_button_variant::OPTICS_UI_BUTTON_SUBTLE,
        active,
        ..Default::default()
    };
    // SAFETY: the frame's context is live inside its begin/end pair, and
    // `c` outlives the call.
    unsafe { optics_ui_sys::optics_ui_button(frame.as_raw(), &opts).clicked }
}

/// Icon for a directory entry (type glyph + MIME / extension mapping).
pub fn entry_icon(e: &Entry) -> AssetId {
    match e.file_type {
        FileType::Directory => dir_icon(&e.name),
        FileType::Symlink => {
            if e.navigable {
                dir_icon(&e.name)
            } else {
                ids::Link2
            }
        }
        _ => {
            let mime = &e.mime_type;
            if mime.starts_with("image/") {
                ids::Image
            } else if mime.starts_with("audio/") {
                ids::Music
            } else if mime.starts_with("video/") {
                ids::Film
            } else if mime == "application/x-desktop" {
                ids::Monitor
            } else if mime == "application/x-executable" || mime == "text/x-script" {
                ids::Terminal
            } else if mime == "application/zip"
                || mime == "application/x-tar"
                || mime == "application/gzip"
                || mime == "application/x-xz"
                || mime == "application/x-7z-compressed"
            {
                ids::Archive
            } else {
                file_icon(&e.name)
            }
        }
    }
}

pub fn bookmark_icon(kind: BookmarkKind) -> AssetId {
    match kind {
        BookmarkKind::Home => ids::Home,
        BookmarkKind::Desktop => ids::Monitor,
        BookmarkKind::Documents => ids::FileText,
        BookmarkKind::Downloads => ids::Download,
        BookmarkKind::Music => ids::Music,
        BookmarkKind::Pictures => ids::Image,
        BookmarkKind::Videos => ids::Film,
        BookmarkKind::Trash => ids::Trash,
        BookmarkKind::Folder => ids::Folder,
    }
}

fn file_icon(name: &str) -> AssetId {
    let lower = name.to_ascii_lowercase();
    let ext = lower.rsplit('.').next().unwrap_or("");
    let icon = match ext {
        "png" | "jpg" | "jpeg" | "gif" | "bmp" | "svg" | "webp" | "ico" => ids::Image,
        "mp3" | "wav" | "ogg" | "flac" | "aac" | "m4a" | "wma" => ids::Music,
        "mp4" | "avi" | "mkv" | "mov" | "wmv" | "flv" | "webm" => ids::Film,
        "zip" | "tar" | "gz" | "bz2" | "xz" | "7z" | "rar" => ids::Archive,
        "c" | "h" | "cpp" | "hpp" | "py" | "js" | "ts" | "rs" | "go" | "java" => ids::Code,
        "sh" | "bash" | "zsh" | "fish" => ids::Terminal,
        "txt" | "md" | "org" | "rst" | "pdf" | "doc" | "docx" | "json" | "xml" | "yaml" | "yml"
        | "toml" | "ini" | "cfg" | "conf" | "log" => ids::FileText,
        _ => ids::File,
    };
    // No extension (or the extension is the whole name, e.g. ".gitignore").
    if ext == lower { ids::File } else { icon }
}

fn dir_icon(name: &str) -> AssetId {
    match name {
        "Desktop" => ids::Monitor,
        "Documents" => ids::FileText,
        "Downloads" => ids::Download,
        "Music" => ids::Music,
        "Pictures" => ids::Image,
        "Videos" => ids::Film,
        ".git" => ids::GitBranch,
        ".config" => ids::Settings,
        _ => ids::Folder,
    }
}

#[cfg(test)]
mod tests {
    //! Behaviour tests for this module.

    use super::*;

    #[test]
    fn every_asset_decodes() {
        for asset in AssetId::ALL {
            let icon = vector::decode(asset.svg()).unwrap_or_else(|e| panic!("{asset:?}: {e}"));
            let covered: u32 = icon.runs.iter().map(|r| r.count).sum();
            assert_eq!(
                covered as usize,
                icon.cmds.len(),
                "{asset:?}: runs cover every command"
            );
        }
    }

    #[test]
    fn all_is_complete_and_ordered() {
        for (i, asset) in AssetId::ALL.into_iter().enumerate() {
            assert_eq!(
                asset as usize, i,
                "ALL must list variants in declaration order"
            );
        }
    }

    #[test]
    fn every_asset_registers() {
        let mut ui = Ui::headless().expect("headless ui");
        let mut set = IconSet::register(&mut ui);
        for asset in AssetId::ALL {
            let id = set.id(asset);
            assert_ne!(
                id,
                optics_ui_icon_id::OPTICS_UI_ICON_NONE,
                "{asset:?} failed to register"
            );
            assert!(
                id.0 >= optics_ui_icon_id::OPTICS_UI_ICON_BUILTIN_COUNT.0,
                "{asset:?} aliases a built-in"
            );
        }
        set.unregister(&mut ui);
        assert_eq!(set, IconSet::EMPTY);
    }

    #[test]
    fn icon_sets_are_per_context() {
        let mut a = Ui::headless().expect("headless ui");
        let mut b = Ui::headless().expect("headless ui");
        let mut set_a = IconSet::register(&mut a);
        let mut set_b = IconSet::register(&mut b);
        assert!(set_a.is_registered_on(&mut a) && set_b.is_registered_on(&mut b));
        assert!(
            !set_a.is_registered_on(&mut b),
            "ids alone must not pass for another context"
        );
        // Unregistering from one context leaves the other's glyphs intact.
        set_a.unregister(&mut a);
        // SAFETY: `b` is live; icon_info only reads its registry.
        let desc = unsafe { optics_ui_sys::optics_ui_icon_info(b.as_raw(), set_b.id(ids::Folder)) };
        assert!(!desc.is_null(), "context b still owns its folder glyph");
        set_b.unregister(&mut b);
    }

    #[test]
    #[should_panic(expected = "not registered on")]
    fn releasing_on_another_context_panics() {
        let mut a = Ui::headless().expect("headless ui");
        let mut b = Ui::headless().expect("headless ui");
        let mut set = IconSet::register(&mut a);
        set.unregister(&mut b);
    }
}
