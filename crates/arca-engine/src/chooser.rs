//! Stable contract and helper types for FileChooser portal operations.
//!
//! Conforms to `org.freedesktop.impl.portal.FileChooser` v3 and the Atrium
//! Arca FileChooser stdio contract v1 (ADR-0027: flat JSON envelopes over
//! anonymous stdin/stdout pipes).

use std::ffi::{OsStr, OsString};
use std::io::{self, Read};
use std::os::unix::ffi::{OsStrExt, OsStringExt};
use std::path::{Component, Path, PathBuf};

/// Version of the `arca --chooser-prompt` stdio contract (matches
/// `atrium-arca-contract::ARCA_CONTRACT_VERSION`).
pub const ARCA_CONTRACT_VERSION: u32 = 1;

/// Maximum serialized byte length accepted on `arca --chooser-prompt`'s stdin
/// (matches `xdg-desktop-portal-atrium`'s `MAX_REQUEST_BYTES` of 1 MiB).
pub const MAX_CHOOSER_REQUEST_BYTES: usize = 1024 * 1024;

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

    /// Whether this filter allows `name`, whose MIME type is `mime`.
    ///
    /// The MIME type is passed in rather than derived from a path: the
    /// listing already classified every entry when it read the directory, and
    /// re-sniffing per frame means one `open`+`read` per file per call (~12 ms
    /// for 2400 entries) inside the render path.
    pub fn allows_mime(&self, name: &str, mime: &str) -> bool {
        if self.rules.is_empty() {
            return true;
        }
        self.rules.iter().any(|rule| match rule.kind {
            FilterRuleKind::Glob => glob_match(&rule.value, name),
            FilterRuleKind::Mime => mime_rule_matches(&rule.value, mime),
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

/// One complete request sent from the D-Bus backend to the chooser.
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

/// Versioned request envelope written to `arca --chooser-prompt`'s stdin.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ChooserRequestEnvelope {
    pub version: u32,
    pub request: FileChooserRequest,
}

/// Versioned response envelope emitted on `arca --chooser-prompt`'s stdout.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ChooserResponseEnvelope {
    pub version: u32,
    pub response: FileChooserResponse,
}

impl ChooserResponseEnvelope {
    #[must_use]
    pub fn new(response: FileChooserResponse) -> Self {
        Self {
            version: ARCA_CONTRACT_VERSION,
            response,
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
            if !matches!(choice.selected.as_str(), "" | "true" | "false") {
                return Err(format!(
                    "boolean choice {:?} selected value must be '', 'true', or 'false'",
                    choice.id
                ));
            }
        } else if !choice.selected.is_empty()
            && !choice.options.iter().any(|(id, _)| id == &choice.selected)
        {
            return Err(format!(
                "choice {:?} initial value {:?} is not among its options",
                choice.id, choice.selected
            ));
        }
    }
    Ok(())
}

/// Check the answered choice values against the offered controls, in order.
fn validate_choice_answers(answers: &[(String, String)], choices: &[Choice]) -> Result<(), String> {
    if answers.len() != choices.len() {
        return Err(format!(
            "chooser returned {} choice answer(s), expected {}",
            answers.len(),
            choices.len()
        ));
    }
    for ((id, value), choice) in answers.iter().zip(choices) {
        if id != &choice.id {
            return Err(format!(
                "chooser returned unexpected choice id {id:?} (expected {:?})",
                choice.id
            ));
        }
        if choice.options.is_empty() {
            if value != "true" && value != "false" {
                return Err(format!("boolean choice {id:?} answered with {value:?}"));
            }
        } else if !choice.options.iter().any(|(opt_id, _)| opt_id == value) {
            return Err(format!(
                "chooser answered choice {id:?} with unoffered option {value:?}"
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
            if let Some(&nc) = name.first()
                && nc.eq_ignore_ascii_case(&literal)
            {
                return glob_recursive(&pattern[1..], &name[1..]);
            }
            false
        }
    }
}

fn parse_class(pattern: &[char]) -> Option<(impl Fn(char) -> bool + use<>, &[char])> {
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

/// Match a known MIME type against a rule pattern (`image/png`, `image/*`).
pub fn mime_rule_matches(rule: &str, mime: &str) -> bool {
    if let Some(prefix) = rule.strip_suffix('*') {
        mime.starts_with(prefix)
    } else {
        mime.eq_ignore_ascii_case(rule)
            || crate::mime::default_database().is_subclass_of(mime, rule)
    }
}

/// Match a path against a MIME type pattern, sniffing the file first.
///
/// Convenience for callers that only have a path. Hot paths (the listing
/// filter) must use [`FileFilter::allows_mime`] with the entry's cached MIME
/// type instead.
pub fn mime_matches(rule: &str, path: &Path) -> bool {
    mime_rule_matches(rule, &crate::mime::guess_mime_type(path))
}

// ---- JSON wire helpers ----------------------------------------------------

/// Strip GTK-style mnemonic markers from a portal label: a lone `_` before a
/// character designates an accelerator and must not be shown literally, while
/// `__` is an escaped literal underscore. The dialog has no accelerators, so
/// keeping the markers would render stray underscores (e.g. `_Open`).
fn strip_mnemonics(label: &str) -> String {
    let mut out = String::with_capacity(label.len());
    let mut chars = label.chars().peekable();
    while let Some(c) = chars.next() {
        if c != '_' {
            out.push(c);
        } else if chars.peek() == Some(&'_') {
            chars.next();
            out.push('_');
        }
    }
    out
}

/// Read and decode one versioned FileChooser request envelope from `reader`.
///
/// The input stream is strictly capped at [`MAX_CHOOSER_REQUEST_BYTES`] so an
/// unbounded pipe cannot exhaust memory. The version stamp is checked before
/// decoding the envelope fields so that a peer speaking another revision (such
/// as the retired v6 multi-prompt envelope) receives a version-mismatch error
/// rather than a missing-field parse failure.
pub fn read_chooser_request<R: io::Read>(reader: R) -> Result<FileChooserRequest, String> {
    let mut bytes = Vec::new();
    reader
        .take((MAX_CHOOSER_REQUEST_BYTES + 1) as u64)
        .read_to_end(&mut bytes)
        .map_err(|e| format!("failed to read chooser request: {e}"))?;
    if bytes.len() > MAX_CHOOSER_REQUEST_BYTES {
        return Err(format!(
            "chooser request exceeds the {MAX_CHOOSER_REQUEST_BYTES}-byte limit"
        ));
    }
    let raw: serde_json::Value = serde_json::from_slice(&bytes)
        .map_err(|e| format!("failed to parse chooser request JSON: {e}"))?;
    let Some(version) = raw.get("version").and_then(serde_json::Value::as_u64) else {
        return Err("chooser request is missing an integer 'version' field".into());
    };
    if version != u64::from(ARCA_CONTRACT_VERSION) {
        return Err(format!(
            "unsupported chooser contract version {version} (this build speaks {ARCA_CONTRACT_VERSION})"
        ));
    }
    let mut envelope: ChooserRequestEnvelope = serde_json::from_value(raw)
        .map_err(|e| format!("failed to decode chooser request envelope: {e}"))?;
    if let Some(label) = envelope.request.accept_label.as_deref() {
        let stripped = strip_mnemonics(label);
        envelope.request.accept_label = (!stripped.is_empty()).then_some(stripped);
    }
    envelope.request.validate()?;
    Ok(envelope.request)
}

/// Write one versioned FileChooser response envelope to `writer` as a single JSON line.
pub fn write_chooser_response<W: io::Write>(
    mut writer: W,
    response: FileChooserResponse,
) -> Result<(), String> {
    let res = ChooserResponseEnvelope::new(response);
    serde_json::to_writer(&mut writer, &res)
        .map_err(|e| format!("failed to serialize chooser response: {e}"))?;
    writer
        .write_all(b"\n")
        .map_err(|e| format!("failed to flush chooser response: {e}"))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A daemon speaking another contract revision must be rejected, not
    /// silently parsed against this build's field meanings.
    #[test]
    fn chooser_rejects_unknown_contract_versions() {
        let request_json = |version: u32| {
            format!(
                r#"{{"version":{version},"request":{{
                    "mode":"open_file","app_id":"x","title":"t","accept_label":null,
                    "modal":false,"parent_window":null,"multiple":false,
                    "current_folder":null,"current_name":null,"current_file":null,
                    "filters":[],"current_filter":null,"choices":[],"files":[]}}}}"#
            )
        };

        assert!(read_chooser_request(request_json(ARCA_CONTRACT_VERSION).as_bytes()).is_ok());
        for version in [0, ARCA_CONTRACT_VERSION + 1, 6, 99] {
            let error = read_chooser_request(request_json(version).as_bytes())
                .expect_err("a foreign contract version must be refused");
            assert!(
                error.contains("contract version"),
                "the error must name the version mismatch, got {error:?}"
            );
        }

        // Retired v6 multi-prompt envelope must also be refused with a version
        // mismatch error before field decoding.
        let legacy_v6 = r#"{"version":6,"prompt":{"kind":"file_chooser","request":{}}}"#;
        let error = read_chooser_request(legacy_v6.as_bytes())
            .expect_err("legacy v6 envelope must be refused");
        assert!(
            error.contains("contract version 6"),
            "legacy v6 refusal must report version 6, got {error:?}"
        );
    }

    #[test]
    fn chooser_rejects_oversized_stdin_before_parsing() {
        let oversized = vec![b' '; MAX_CHOOSER_REQUEST_BYTES + 1];
        let error = read_chooser_request(oversized.as_slice())
            .expect_err("oversized stdin must be refused");
        assert!(
            error.contains("exceeds"),
            "error must report size bound, got {error:?}"
        );
    }

    #[test]
    fn chooser_rejects_unknown_envelope_fields() {
        let with_extra = format!(
            r#"{{"version":{ARCA_CONTRACT_VERSION},"request":{{
                "mode":"open_file","app_id":"x","title":"t","accept_label":null,
                "modal":false,"parent_window":null,"multiple":false,
                "current_folder":null,"current_name":null,"current_file":null,
                "filters":[],"current_filter":null,"choices":[],"files":[]}},
                "appearance":null}}"#
        );
        let error = read_chooser_request(with_extra.as_bytes())
            .expect_err("unknown top-level fields must be rejected");
        assert!(
            error.contains("unknown field"),
            "expected deny_unknown_fields error, got {error:?}"
        );
    }

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
        // `allows_mime` takes the MIME type the directory listing already
        // resolved, so filtering never touches the filesystem.
        assert!(filter.allows_mime("photo.png", "image/png"));
        assert!(filter.allows_mime("photo.jpg", "image/jpeg"));
        assert!(!filter.allows_mime("doc.pdf", "application/pdf"));
        // A directory entry is navigable and must never be filtered out.
        assert!(!filter.allows_mime("album", "inode/directory"));
    }

    #[test]
    fn mnemonics_are_stripped_from_accept_label() {
        assert_eq!(strip_mnemonics("_Open"), "Open");
        assert_eq!(strip_mnemonics("Save _As"), "Save As");
        assert_eq!(strip_mnemonics("A__B"), "A_B");
        assert_eq!(strip_mnemonics("Plain"), "Plain");
        assert_eq!(strip_mnemonics("_"), "");

        let req = FileChooserRequest {
            mode: FileChooserMode::OpenFile,
            app_id: "org.example.App".into(),
            title: "Open File".into(),
            accept_label: Some("_Open".into()),
            modal: false,
            parent_window: None,
            multiple: false,
            current_folder: Some(BytePath::from_path("/home/user")),
            current_name: None,
            current_file: None,
            filters: Vec::new(),
            current_filter: None,
            choices: Vec::new(),
            files: Vec::new(),
        };
        let json = serde_json::to_string(&ChooserRequestEnvelope {
            version: ARCA_CONTRACT_VERSION,
            request: req.clone(),
        })
        .unwrap();
        let decoded = read_chooser_request(json.as_bytes()).unwrap();
        assert_eq!(decoded.accept_label.as_deref(), Some("Open"));

        // A lone mnemonic marker normalizes to None so the UI uses its default label.
        let mut bare = req;
        bare.accept_label = Some("_".into());
        let json = serde_json::to_string(&ChooserRequestEnvelope {
            version: ARCA_CONTRACT_VERSION,
            request: bare,
        })
        .unwrap();
        let decoded = read_chooser_request(json.as_bytes()).unwrap();
        assert_eq!(decoded.accept_label, None);
    }

    #[test]
    fn empty_initial_choice_selection_is_valid_and_answers_must_match_order() {
        let req = FileChooserRequest {
            mode: FileChooserMode::OpenFile,
            app_id: "org.example.App".into(),
            title: "Open".into(),
            accept_label: None,
            modal: false,
            parent_window: None,
            multiple: false,
            current_folder: None,
            current_name: None,
            current_file: None,
            filters: Vec::new(),
            current_filter: None,
            choices: vec![
                Choice {
                    id: "read_only".into(),
                    label: "Read only".into(),
                    options: Vec::new(),
                    selected: String::new(),
                },
                Choice {
                    id: "encoding".into(),
                    label: "Encoding".into(),
                    options: vec![
                        ("utf8".into(), "UTF-8".into()),
                        ("latin1".into(), "Latin-1".into()),
                    ],
                    selected: String::new(),
                },
            ],
            files: Vec::new(),
        };
        assert!(req.validate().is_ok());

        let ordered = FileChooserResponse::Selected {
            paths: vec![BytePath::from_path("/tmp/doc.txt")],
            current_filter: None,
            choices: vec![
                ("read_only".into(), "false".into()),
                ("encoding".into(), "utf8".into()),
            ],
        };
        assert!(ordered.validate_for(&req).is_ok());

        let reordered = FileChooserResponse::Selected {
            paths: vec![BytePath::from_path("/tmp/doc.txt")],
            current_filter: None,
            choices: vec![
                ("encoding".into(), "utf8".into()),
                ("read_only".into(), "false".into()),
            ],
        };
        assert!(reordered.validate_for(&req).is_err());
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
        let envelope = ChooserRequestEnvelope {
            version: ARCA_CONTRACT_VERSION,
            request: req.clone(),
        };

        let json = serde_json::to_string(&envelope).unwrap();
        let value: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(value["version"], ARCA_CONTRACT_VERSION);
        assert!(value["request"].is_object());
        assert!(value.get("prompt").is_none());

        let decoded_fc = read_chooser_request(json.as_bytes()).unwrap();
        assert_eq!(decoded_fc.app_id, req.app_id);
        assert_eq!(decoded_fc.title, req.title);

        let response = FileChooserResponse::Selected {
            paths: vec![BytePath::from_path("/home/user/document.txt")],
            current_filter: None,
            choices: vec![("read_only".into(), "true".into())],
        };
        let mut resp_buf = Vec::new();
        write_chooser_response(&mut resp_buf, response).unwrap();

        let resp_value: serde_json::Value = serde_json::from_slice(&resp_buf).unwrap();
        assert_eq!(resp_value["version"], ARCA_CONTRACT_VERSION);
        assert!(resp_value["response"].is_object());
        assert!(resp_value.get("result").is_none());

        let resp_parsed: ChooserResponseEnvelope = serde_json::from_slice(&resp_buf).unwrap();
        assert_eq!(resp_parsed.version, ARCA_CONTRACT_VERSION);
    }
}
