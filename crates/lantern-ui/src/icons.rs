//! Icon glyphs for files, folders and chrome.
//!
//! Lantern maintains its own icon set in `assets/icons/*.svg` (see the
//! README there). Each glyph is registered with lens on first use
//! (`lens_icon_register_svg` — the runtime counterpart of lens's baked-in
//! table) and drawn through the native icon widgets: vector-crisp at any
//! size, theme-tinted, no raster assets or generator scripts involved.

use std::sync::OnceLock;

use iris::Frame;
use lantern_core::bookmarks::BookmarkKind;
use lantern_core::entry::{Entry, FileType};

/// One glyph of lantern's icon set; the variant name is the PascalCase of
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
    StarRounded,
    StarRoundedFilled,
    Sun,
    Terminal,
    X,
}

pub(crate) mod ids {
    //! Keeps call sites reading `ids::X`.
    #[allow(unused_imports)]
    pub(crate) use super::AssetId::*;
}

impl AssetId {
    pub(crate) const ALL: &'static [AssetId] = &[
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
        AssetId::StarRounded,
        AssetId::StarRoundedFilled,
        AssetId::Sun,
        AssetId::Terminal,
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
            AssetId::StarRounded => include_str!("../../../assets/icons/star-rounded.svg"),
            AssetId::StarRoundedFilled => include_str!("../../../assets/icons/star-rounded-filled.svg"),
            AssetId::Sun => include_str!("../../../assets/icons/sun.svg"),
            AssetId::Terminal => include_str!("../../../assets/icons/terminal.svg"),
            AssetId::X => include_str!("../../../assets/icons/x.svg"),
        }
    }

    fn index(self) -> usize {
        self as usize
    }
}

/// The process-global registration table, built once on first use.
static REGISTERED: OnceLock<Vec<lens_sys::lens_icon_id>> = OnceLock::new();

/// The lens id for an asset, registering the whole set on first call. A
/// failed parse yields the invalid id (widgets then draw nothing); the
/// `all_icon_assets_register` test keeps that from shipping.
pub(crate) fn lens_id(id: AssetId) -> lens_sys::lens_icon_id {
    let ids = REGISTERED.get_or_init(|| {
        AssetId::ALL
            .iter()
            .map(|asset| {
                // LENS_ICON_INVALID is (lens_icon_id)-1.
                lens::register_svg_icon(asset.svg())
                    .unwrap_or(lens_sys::lens_icon_id(u32::MAX))
            })
            .collect()
    });
    ids[id.index()]
}

pub fn icon(frame: &mut Frame, id: AssetId, size: f32) {
    frame.icon_raw(lens_id(id), size);
}

/// Ghost icon button in a square `box_size` hit target.
pub fn icon_button(frame: &mut Frame, id: AssetId, box_size: f32) -> bool {
    let id_str = format!("##icon-{id:?}");
    let c = std::ffi::CString::new(id_str).unwrap();
    let opts = lens_sys::lens_button_opts {
        box_: lens_sys::lens_box {
            id: c.as_ptr(),
            width: box_size,
            height: box_size,
            ..Default::default()
        },
        icon: lens_id(id),
        ..Default::default()
    };
    unsafe { lens_sys::lens_button(frame.as_raw(), &opts).clicked }
}

/// Icon button whose active state is a rounded accent-tinted chip, used for
/// the segmented view-mode toggle.
pub fn icon_button_active_rounded(
    frame: &mut Frame,
    id: AssetId,
    box_size: f32,
    active: bool,
) -> bool {
    let id_str = format!("##icon-active-{id:?}");
    let c = std::ffi::CString::new(id_str).unwrap();
    let opts = lens_sys::lens_button_opts {
        box_: lens_sys::lens_box {
            id: c.as_ptr(),
            width: box_size,
            height: box_size,
            ..Default::default()
        },
        icon: lens_id(id),
        active,
        ..Default::default()
    };
    unsafe { lens_sys::lens_button(frame.as_raw(), &opts).clicked }
}

/// Icon button that swaps glyph with its checked state (eye/eye-off, star…).
pub fn icon_toggle_button(
    frame: &mut Frame,
    unchecked: AssetId,
    checked: AssetId,
    box_size: f32,
    is_checked: bool,
) -> bool {
    let icon_id = if is_checked { checked } else { unchecked };
    let id_str = format!("##icon-toggle-{unchecked:?}-{checked:?}");
    let c = std::ffi::CString::new(id_str).unwrap();
    let opts = lens_sys::lens_button_opts {
        box_: lens_sys::lens_box {
            id: c.as_ptr(),
            width: box_size,
            height: box_size,
            ..Default::default()
        },
        icon: lens_id(icon_id),
        active: is_checked,
        ..Default::default()
    };
    unsafe { lens_sys::lens_button(frame.as_raw(), &opts).clicked }
}

/// Full-width sidebar/menu row with a leading icon.
pub fn selectable_icon(frame: &mut Frame, id: AssetId, label: &str, selected: bool) -> bool {
    frame.selectable_icon(label, lens_id(id), selected)
}

/// Icon for a directory entry (type glyph + extension mapping).
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
        _ => file_icon(&e.name),
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
