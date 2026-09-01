//! Stable contract and helper types for FileChooser portal operations.
//!
//! Conforms to `org.freedesktop.impl.portal.FileChooser` v3 and the Aegis
//! portal prompter process protocol (JSON over anonymous stdin/stdout pipes).

use std::ffi::{OsStr, OsString};
use std::io;
use std::os::unix::ffi::{OsStrExt, OsStringExt};
use std::path::{Component, Path, PathBuf};

/// Version of the private stdin/stdout process contract (matches Aegis
/// prompter contract v6).
pub const PROCESS_CONTRACT_VERSION: u32 = 6;

/// One filesystem path encoded as native Unix bytes.
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(transparent)]
pub struct BytePath(pub Vec<u8>);

impl BytePath {
    #[must_use]
    pub fn from_path(path: impl AsRef<Path>) -> Self {
        Self(path.as_ref().as_os_str().as_bytes().to_vec())
    }

    #[must_use]
    pub fn to_path_buf(&self) -> PathBuf {
        PathBuf::from(OsString::from_vec(self.0.clone()))
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

impl From<PathBuf> for BytePath {
    fn from(path: PathBuf) -> Self {
        Self::from_path(path)
    }
}

impl From<&Path> for BytePath {
    fn from(path: &Path) -> Self {
        Self::from_path(path)
    }
}

impl From<&str> for BytePath {
    fn from(path: &str) -> Self {
        Self::from_path(Path::new(path))
    }
}

/// The FileChooser operation mode requested by the portal.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FileChooserMode {
    OpenFile,
    OpenDirectory,
    SaveFile,
    SaveFiles,
}

/// The two rule kinds in the portal's `(sa(us))` filter structure.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FilterRuleKind {
    Glob,
    Mime,
}

/// One typed file-filter rule.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct FilterRule {
    pub kind: FilterRuleKind,
    pub value: String,
}

/// One user-visible file filter option.
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct FileFilter {
    pub label: String,
    pub rules: Vec<FilterRule>,
}

impl FileFilter {
    pub fn new(label: impl Into<String>, rules: Vec<FilterRule>) -> Self {
        Self {
            label: label.into(),
            rules,
        }
    }

    /// Whether this filter allows the given file name / path.
    pub fn allows(&self, name: &str, path: &Path) -> bool {
        if self.rules.is_empty() {
            return true;
        }
        self.rules.iter().any(|rule| match rule.kind {
            FilterRuleKind::Glob => glob_match(&rule.value, name),
            FilterRuleKind::Mime => mime_matches(&rule.value, path),
        })
    }
}

/// One optional choice control embedded in a FileChooser request.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Choice {
    pub id: String,
    pub label: String,
    /// Empty means a boolean check button whose values are `true`/`false`.
    pub options: Vec<(String, String)>,
    pub selected: String,
}

/// The compositor appearance snapshot for dialog styling.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PromptAppearance {
    pub color_scheme: PromptColorScheme,
    pub accent_color: Option<PromptAccent>,
    pub high_contrast: bool,
    pub reduced_motion: bool,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum PromptColorScheme {
    #[default]
    System,
    Dark,
    Light,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct PromptAccent {
    pub red: u8,
    pub green: u8,
    pub blue: u8,
}

/// One complete request sent from the D-Bus backend to the prompter.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct FileChooserRequest {
    pub mode: FileChooserMode,
    pub app_id: String,
    pub title: String,
    pub accept_label: Option<String>,
    pub modal: bool,
    pub parent_window: Option<String>,
    pub multiple: bool,
    pub current_folder: Option<BytePath>,
    pub current_name: Option<String>,
    pub current_file: Option<BytePath>,
    pub filters: Vec<FileFilter>,
    pub current_filter: Option<FileFilter>,
    pub choices: Vec<Choice>,
    /// Suggested basenames for `SaveFiles`, in request order.
    pub files: Vec<BytePath>,
}

impl FileChooserRequest {
    /// Reject malformed values before any dialog or filesystem access.
    pub fn validate(&self) -> Result<(), String> {
        for (name, path) in [
            ("current_folder", self.current_folder.as_ref()),
            ("current_file", self.current_file.as_ref()),
        ] {
            if let Some(path) = path {
                validate_absolute_path(name, &path.to_path_buf())?;
            }
        }
        if self.mode != FileChooserMode::SaveFiles && !self.files.is_empty() {
            return Err("suggested files are valid only for SaveFiles".into());
        }
        if self.mode == FileChooserMode::SaveFiles && self.files.is_empty() {
            return Err("SaveFiles requires at least one suggested basename".into());
        }
        for name in &self.files {
            validate_basename(&name.to_path_buf())?;
        }
        for filter in self.filters.iter().chain(self.current_filter.as_ref()) {
            if filter.label.is_empty() || filter.rules.iter().any(|rule| rule.value.is_empty()) {
                return Err("filter labels and rules must not be empty".into());
            }
        }
        validate_choices(&self.choices)
    }

    /// Apply `SaveFiles` basename and collision semantics to the selected
    /// folder. Other modes return the selected paths unchanged.
    pub fn finish_paths(&self, selected: Vec<PathBuf>) -> Result<Vec<PathBuf>, String> {
        if self.mode != FileChooserMode::SaveFiles {
            return Ok(selected);
        }
        let folder = selected
            .into_iter()
            .next()
            .ok_or_else(|| "SaveFiles returned no selected folder".to_owned())?;
        let mut reserved = std::collections::HashSet::new();
        let mut paths = Vec::with_capacity(self.files.len());
        for name in &self.files {
            let path = unique_child(&folder, &name.to_path_buf(), &reserved)?;
            reserved.insert(path.clone());
            paths.push(path);
        }
        Ok(paths)
    }
}

/// The one response emitted by a prompter process.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum FileChooserResponse {
    Selected {
        paths: Vec<BytePath>,
        current_filter: Option<FileFilter>,
        choices: Vec<(String, String)>,
    },
    Cancelled,
    Failed {
        message: String,
    },
}

impl FileChooserResponse {
    /// Validate the child result against the exact request.
    pub fn validate_for(&self, request: &FileChooserRequest) -> Result<(), String> {
        let Self::Selected {
            paths,
            current_filter,
            choices,
        } = self
        else {
            return Ok(());
        };

        let expected_paths = match request.mode {
            FileChooserMode::SaveFile => Some(1),
            FileChooserMode::SaveFiles => Some(request.files.len()),
            FileChooserMode::OpenFile | FileChooserMode::OpenDirectory if !request.multiple => {
                Some(1)
            }
            FileChooserMode::OpenFile | FileChooserMode::OpenDirectory => None,
        };
        if paths.is_empty() || expected_paths.is_some_and(|expected| paths.len() != expected) {
            return Err(format!(
                "prompter returned {} path(s), incompatible with {:?}",
                paths.len(),
                request.mode
            ));
        }
        for path in paths {
            let path = path.to_path_buf();
            if !path.is_absolute() || path.as_os_str().as_bytes().contains(&0) {
                return Err(format!("prompter returned an invalid local path {path:?}"));
            }
        }

        if let Some(filter) = current_filter {
            let offered = request.filters.iter().any(|candidate| candidate == filter)
                || (request.filters.is_empty() && request.current_filter.as_ref() == Some(filter));
            if !offered {
                return Err("prompter returned a filter that was not offered".into());
            }
        }

        validate_choice_answers(choices, &request.choices)
    }
}

/// Envelope for prompter requests (versioned wire format).
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct PrompterRequest {
    pub version: u32,
    pub prompt: PromptRequest,
    #[serde(default)]
    pub appearance: Option<PromptAppearance>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PromptRequest {
    FileChooser(FileChooserRequest),
    #[serde(other)]
    Unknown,
}

/// Envelope for prompter responses (versioned wire format).
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct PrompterResponse {
    pub version: u32,
    pub result: PromptResult,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PromptResult {
    FileChooser(FileChooserResponse),
}

impl PrompterResponse {
    #[must_use]
    pub fn new(response: FileChooserResponse) -> Self {
        Self {
            version: PROCESS_CONTRACT_VERSION,
            result: PromptResult::FileChooser(response),
        }
    }
}

// ---- Validation helpers ---------------------------------------------------

fn validate_basename(path: &Path) -> Result<(), String> {
    if path.as_os_str().as_bytes().contains(&0) {
        return Err("SaveFiles basenames must not contain NUL".into());
    }
    let mut components = path.components();
    match (components.next(), components.next()) {
        (Some(Component::Normal(name)), None) if !name.is_empty() => Ok(()),
        _ => Err(format!(
            "SaveFiles name {path:?} is not a single non-empty basename"
        )),
    }
}

fn validate_absolute_path(name: &str, path: &Path) -> Result<(), String> {
    if !path.is_absolute() || path.as_os_str().as_bytes().contains(&0) {
        return Err(format!("{name} is not a valid absolute Unix path"));
    }
    Ok(())
}

fn validate_choices(choices: &[Choice]) -> Result<(), String> {
    let mut ids = std::collections::BTreeSet::new();
    for choice in choices {
        if choice.id.is_empty() || choice.label.is_empty() {
            return Err("choice ids and labels must not be empty".into());
        }
        if !ids.insert(choice.id.as_str()) {
            return Err(format!("duplicate choice id {:?}", choice.id));
        }
        if choice
            .options
            .iter()
            .any(|(id, label)| id.is_empty() || label.is_empty())
        {
            return Err(format!("choice {:?} contains an empty option", choice.id));
        }
        let mut option_ids = std::collections::BTreeSet::new();
        if choice
            .options
            .iter()
            .any(|(id, _)| !option_ids.insert(id.as_str()))
        {
            return Err(format!(
                "choice {:?} contains duplicate option ids",
                choice.id
            ));
        }
        if choice.options.is_empty() {
            if choice.selected != "true" && choice.selected != "false" {
                return Err(format!(
                    "boolean choice {:?} selected value must be 'true' or 'false'",
                    choice.id
                ));
            }
        } else if !choice
            .options
            .iter()
            .any(|(id, _)| id == &choice.selected)
        {
            return Err(format!(
                "choice {:?} initial value {:?} is not among its options",
                choice.id, choice.selected
            ));
        }
    }
    Ok(())
}

fn validate_choice_answers(
    answers: &[(String, String)],
    choices: &[Choice],
) -> Result<(), String> {
    if answers.len() != choices.len() {
        return Err(format!(
            "prompter returned {} choice answer(s), expected {}",
            answers.len(),
            choices.len()
        ));
    }
    let mut seen = std::collections::BTreeSet::new();
    for (id, value) in answers {
        if !seen.insert(id.as_str()) {
            return Err(format!("prompter returned duplicate answer for {id:?}"));
        }
        let Some(choice) = choices.iter().find(|c| &c.id == id) else {
            return Err(format!("prompter answered unoffered choice {id:?}"));
        };
        if choice.options.is_empty() {
            if value != "true" && value != "false" {
                return Err(format!("boolean choice {id:?} answered with {value:?}"));
            }
        } else if !choice.options.iter().any(|(opt_id, _)| opt_id == value) {
            return Err(format!(
                "prompter answered choice {id:?} with unoffered option {value:?}"
            ));
        }
    }
    Ok(())
}

fn unique_child(
    folder: &Path,
    name: &Path,
    reserved: &std::collections::HashSet<PathBuf>,
) -> Result<PathBuf, String> {
    validate_basename(name)?;
    let candidate = folder.join(name);
    if !reserved.contains(&candidate) && !path_occupied(&candidate) {
        return Ok(candidate);
    }

    let raw = name.as_os_str().as_bytes();
    let (stem, extension) = split_extension(raw);
    for suffix in 1..=u32::MAX {
        let mut bytes = stem.to_vec();
        bytes.extend_from_slice(format!(" ({suffix})").as_bytes());
        if let Some(extension) = extension {
            bytes.push(b'.');
            bytes.extend_from_slice(extension);
        }
        let candidate = folder.join(OsStr::from_bytes(&bytes));
        if !reserved.contains(&candidate) && !path_occupied(&candidate) {
            return Ok(candidate);
        }
    }
    Err(format!(
        "could not construct a unique filename for {name:?}"
    ))
}

fn path_occupied(path: &Path) -> bool {
    match std::fs::symlink_metadata(path) {
        Ok(_) => true,
        Err(error) if error.kind() == io::ErrorKind::NotFound => false,
        Err(_) => true,
    }
}

fn split_extension(name: &[u8]) -> (&[u8], Option<&[u8]>) {
    let Some(dot) = name.iter().rposition(|byte| *byte == b'.') else {
        return (name, None);
    };
    if dot == 0 || dot + 1 == name.len() {
        return (name, None);
    }
    (&name[..dot], Some(&name[dot + 1..]))
}

// ---- Glob & MIME matching -------------------------------------------------

/// Match a filename against a glob pattern. Supports `*`, `?`, `[...]` character
/// sets and ranges, and `[!...]` negation. Matching is case-insensitive for ASCII.
pub fn glob_match(pattern: &str, name: &str) -> bool {
    let p: Vec<char> = pattern.chars().collect();
    let n: Vec<char> = name.chars().collect();
    glob_recursive(&p, &n)
}

fn glob_recursive(pattern: &[char], name: &[char]) -> bool {
    match pattern.first() {
        None => name.is_empty(),
        Some('*') => {
            glob_recursive(&pattern[1..], name)
                || (!name.is_empty() && glob_recursive(pattern, &name[1..]))
        }
        Some('?') => !name.is_empty() && glob_recursive(&pattern[1..], &name[1..]),
        Some('[') => match parse_class(pattern) {
            Some((matches, rest)) if !name.is_empty() => {
                matches(name[0]) && glob_recursive(rest, &name[1..])
            }
            _ => {
                name.first().map(|c| c.to_ascii_lowercase()) == Some('[')
                    && glob_recursive(&pattern[1..], &name[1..])
            }
        },
        Some(&literal) => {
            if let Some(&nc) = name.first() {
                if nc.to_ascii_lowercase() == literal.to_ascii_lowercase() {
                    return glob_recursive(&pattern[1..], &name[1..]);
                }
            }
            false
        }
    }
}

fn parse_class(pattern: &[char]) -> Option<(impl Fn(char) -> bool + 'static, &[char])> {
    debug_assert_eq!(pattern.first(), Some(&'['));
    let mut items: Vec<(char, char)> = Vec::new();
    let mut index = 1;
    let negated = pattern.get(index) == Some(&'!');
    if negated {
        index += 1;
    }
    while index < pattern.len() && pattern[index] != ']' {
        let low = pattern[index].to_ascii_lowercase();
        let high = if pattern.get(index + 1) == Some(&'-')
            && pattern.get(index + 2).is_some_and(|&c| c != ']')
        {
            index += 2;
            pattern[index].to_ascii_lowercase()
        } else {
            low
        };
        items.push((low, high));
        index += 1;
    }
    if index >= pattern.len() || items.is_empty() {
        return None;
    }
    Some((
        move |c: char| {
            let lower = c.to_ascii_lowercase();
            items
                .iter()
                .any(|&(low, high)| (low..=high).contains(&lower))
                != negated
        },
        &pattern[index + 1..],
    ))
}

/// Match path against a MIME type pattern (e.g. `image/png` or wildcard `image/*`).
pub fn mime_matches(rule: &str, path: &Path) -> bool {
    let ext = path
        .extension()
        .and_then(OsStr::to_str)
        .unwrap_or("")
        .to_ascii_lowercase();
    if ext.is_empty() {
        return false;
    }
    let mime = extension_to_mime(&ext);
    if let Some(prefix) = rule.strip_suffix('*') {
        mime.starts_with(prefix)
    } else {
        mime.eq_ignore_ascii_case(rule)
    }
}

fn extension_to_mime(ext: &str) -> &'static str {
    match ext {
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "svg" => "image/svg+xml",
        "bmp" => "image/bmp",
        "ico" => "image/x-icon",
        "tiff" | "tif" => "image/tiff",
        "avif" => "image/avif",
        "mp3" => "audio/mpeg",
        "flac" => "audio/flac",
        "wav" => "audio/wav",
        "ogg" | "oga" => "audio/ogg",
        "m4a" => "audio/mp4",
        "aac" => "audio/aac",
        "opus" => "audio/opus",
        "mp4" | "m4v" => "video/mp4",
        "mkv" => "video/x-matroska",
        "webm" => "video/webm",
        "avi" => "video/x-msvideo",
        "mov" => "video/quicktime",
        "txt" => "text/plain",
        "md" | "markdown" => "text/markdown",
        "rs" => "text/rust",
        "c" | "h" => "text/x-c",
        "cpp" | "hpp" | "cc" => "text/x-c++",
        "py" => "text/x-python",
        "js" => "text/javascript",
        "ts" => "text/typescript",
        "html" | "htm" => "text/html",
        "css" => "text/css",
        "json" => "application/json",
        "toml" => "application/toml",
        "yaml" | "yml" => "application/yaml",
        "xml" => "application/xml",
        "pdf" => "application/pdf",
        "zip" => "application/zip",
        "tar" => "application/x-tar",
        "gz" => "application/gzip",
        "sh" => "application/x-sh",
        _ => "application/octet-stream",
    }
}

// ---- JSON wire helpers ----------------------------------------------------

/// Read and decode one request envelope from `reader`.
pub fn read_prompter_request<R: io::Read>(
    reader: R,
) -> Result<(FileChooserRequest, Option<PromptAppearance>), String> {
    let req: PrompterRequest = serde_json::from_reader(reader)
        .map_err(|e| format!("failed to parse prompter request JSON: {e}"))?;
    match req.prompt {
        PromptRequest::FileChooser(fc) => {
            fc.validate()?;
            Ok((fc, req.appearance))
        }
        _ => Err("request did not contain a file chooser prompt".into()),
    }
}

/// Write one response envelope to `writer` as JSON.
pub fn write_prompter_response<W: io::Write>(
    mut writer: W,
    response: FileChooserResponse,
) -> Result<(), String> {
    let res = PrompterResponse::new(response);
    serde_json::to_writer(&mut writer, &res)
        .map_err(|e| format!("failed to serialize prompter response: {e}"))?;
    writer
        .write_all(b"\n")
        .map_err(|e| format!("failed to flush prompter response: {e}"))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn glob_matching() {
        assert!(glob_match("*.png", "image.png"));
        assert!(glob_match("*.png", "IMAGE.PNG"));
        assert!(glob_match("*.jpg", "photo.JPG"));
        assert!(glob_match("*.tar.gz", "archive.tar.gz"));
        assert!(!glob_match("*.png", "photo.jpg"));
        assert!(glob_match("file-?.txt", "file-1.txt"));
        assert!(!glob_match("file-?.txt", "file-12.txt"));
        assert!(glob_match("[a-z]*.txt", "alpha.txt"));
        assert!(!glob_match("[!a-z]*.txt", "alpha.txt"));
    }

    #[test]
    fn mime_matching() {
        assert!(mime_matches("image/png", Path::new("pic.png")));
        assert!(mime_matches("image/*", Path::new("pic.png")));
        assert!(mime_matches("image/*", Path::new("pic.jpg")));
        assert!(!mime_matches("video/*", Path::new("pic.png")));
        assert!(mime_matches("text/plain", Path::new("notes.txt")));
        assert!(mime_matches("application/pdf", Path::new("doc.pdf")));
    }

    #[test]
    fn filter_rule_allows() {
        let filter = FileFilter::new(
            "Images",
            vec![
                FilterRule {
                    kind: FilterRuleKind::Glob,
                    value: "*.png".into(),
                },
                FilterRule {
                    kind: FilterRuleKind::Mime,
                    value: "image/jpeg".into(),
                },
            ],
        );
        assert!(filter.allows("photo.png", Path::new("/tmp/photo.png")));
        assert!(filter.allows("photo.jpg", Path::new("/tmp/photo.jpg")));
        assert!(!filter.allows("doc.pdf", Path::new("/tmp/doc.pdf")));
    }

    #[test]
    fn wire_roundtrip() {
        let req = FileChooserRequest {
            mode: FileChooserMode::OpenFile,
            app_id: "org.example.App".into(),
            title: "Open File".into(),
            accept_label: Some("Open".into()),
            modal: true,
            parent_window: None,
            multiple: false,
            current_folder: Some(BytePath::from_path("/home/user")),
            current_name: None,
            current_file: None,
            filters: vec![FileFilter::new(
                "All Files",
                vec![FilterRule {
                    kind: FilterRuleKind::Glob,
                    value: "*".into(),
                }],
            )],
            current_filter: None,
            choices: vec![Choice {
                id: "read_only".into(),
                label: "Read Only".into(),
                options: Vec::new(),
                selected: "false".into(),
            }],
            files: Vec::new(),
        };
        let prompter_req = PrompterRequest {
            version: PROCESS_CONTRACT_VERSION,
            prompt: PromptRequest::FileChooser(req.clone()),
            appearance: Some(PromptAppearance {
                color_scheme: PromptColorScheme::Dark,
                accent_color: Some(PromptAccent {
                    red: 88,
                    green: 101,
                    blue: 242,
                }),
                high_contrast: false,
                reduced_motion: false,
            }),
        };

        let json = serde_json::to_string(&prompter_req).unwrap();
        let (decoded_fc, appearance) = read_prompter_request(json.as_bytes()).unwrap();
        assert_eq!(decoded_fc.app_id, req.app_id);
        assert_eq!(decoded_fc.title, req.title);
        assert_eq!(
            appearance.unwrap().color_scheme,
            PromptColorScheme::Dark
        );

        let response = FileChooserResponse::Selected {
            paths: vec![BytePath::from_path("/home/user/document.txt")],
            current_filter: None,
            choices: vec![("read_only".into(), "true".into())],
        };
        let mut resp_buf = Vec::new();
        write_prompter_response(&mut resp_buf, response).unwrap();

        let resp_parsed: PrompterResponse = serde_json::from_slice(&resp_buf).unwrap();
        assert_eq!(resp_parsed.version, PROCESS_CONTRACT_VERSION);
    }
}
