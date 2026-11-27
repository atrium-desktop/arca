//! Command-line parsing for the `arca` binary.
//!
//! Kept pure and separate from `main` so every flag combination is testable
//! without spawning a process or opening a window. `main` performs the side
//! effects the parsed [`Command`] describes.

use arca_engine::ViewMode;

/// Where the initial directory comes from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StartDir {
    /// No path given: the process working directory.
    WorkingDirectory,
    /// An explicit path (argument order preserved for last-wins).
    Path(String),
}

/// What the process should do.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    /// Print help and exit 0.
    Help,
    /// Print the version and exit 0.
    Version,
    /// Print the latest telemetry snapshot of the window process.
    Diagnostics,
    /// Read a portal request from stdin, prompt, write the response.
    Prompter,
    /// Run a standalone picker.
    Chooser(ChooserInvocation),
    /// Forward `path` to a running instance instead of opening a window.
    Forward { path: String },
    /// Open the interactive file manager window.
    Window {
        view_mode: Option<ViewMode>,
        start: StartDir,
    },
}

/// A standalone file-picker invocation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChooserInvocation {
    pub mode: arca_engine::chooser::FileChooserMode,
    pub multiple: bool,
    pub title: Option<String>,
    /// Suggested filename (`--save-file --name`).
    pub name: Option<String>,
    pub start_dir: Option<String>,
    /// Emit raw Unix path bytes delimited by NUL (`\0`) instead of newlines (`-0`/`--null`).
    pub null_delimited: bool,
}

/// The `--help` text.
pub const USAGE: &str = "Arca file manager

Usage:
  arca [--new-window] [--grid|--list|--miller] [--] [DIRECTORY]
  arca --chooser-prompt | arca --prompter
  arca --choose-file [-0|--null] [--title TITLE] [--] [DIRECTORY]
  arca --choose-files [-0|--null] [--title TITLE] [--] [DIRECTORY]
  arca --choose-dir [-0|--null] [--title TITLE] [--] [DIRECTORY]
  arca --save-file [-0|--null] [--name NAME] [--title TITLE] [--] [DIRECTORY]

Options:
  -h, --help       Print this help and exit
  -V, --version    Print the version and exit
  -0, --null       Terminate standalone picker output paths with NUL bytes
  --new-window     Open a new window instead of forwarding to a running instance
  --diagnostics    Print latency, counters, and recent alerts from the window
                   (see also $XDG_STATE_HOME/arca/logs/arca.log)
";

/// Flags that take a value and would otherwise be mistaken for a directory.
const VALUE_FLAGS: [&str; 2] = ["--title", "--name"];

impl Command {
    /// Parse `args` (without `argv[0]`).
    ///
    /// `forward_probe` is asked whether an already-running instance accepts
    /// `path`; injecting it keeps this function pure.
    pub fn parse(args: &[String], forward_probe: impl Fn(&str) -> bool) -> Command {
        let pre_dash = pre_double_dash(args);

        // Mode selection is resolved before anything else: a prompter or
        // picker invocation must never fall through to window handling.
        // Arguments after `--` are always positional paths, never flags.
        if pre_dash
            .iter()
            .any(|a| a == "--chooser-prompt" || a == "--prompter")
        {
            return Command::Prompter;
        }
        if let Some(first) = pre_dash.first()
            && let Some(invocation) = parse_chooser(first, &args[1..])
        {
            return Command::Chooser(invocation);
        }

        if pre_dash.iter().any(|a| a == "-h" || a == "--help") {
            return Command::Help;
        }
        if pre_dash.iter().any(|a| a == "-V" || a == "--version") {
            return Command::Version;
        }
        // Read-only and local: never opens a window or forwards.
        if pre_dash.iter().any(|a| a == "--diagnostics") {
            return Command::Diagnostics;
        }

        let mut view_mode = None;
        let mut start = StartDir::WorkingDirectory;
        let mut after_double_dash = false;
        let mut iter = args.iter();
        while let Some(argument) = iter.next() {
            if after_double_dash {
                start = StartDir::Path(normalize_cli_target(argument));
                continue;
            }
            match argument.as_str() {
                "--" => after_double_dash = true,
                // Suppresses the single-instance hand-off.
                "--new-window" => {}
                "--grid" => view_mode = Some(ViewMode::Grid),
                "--list" => view_mode = Some(ViewMode::List),
                "--miller" => view_mode = Some(ViewMode::Miller),
                // Consume the value belonging to a value-taking flag so it is
                // never mistaken for a start directory.
                value if VALUE_FLAGS.contains(&value) => {
                    let _ = iter.next();
                }
                path if !path.starts_with('-') => {
                    start = StartDir::Path(normalize_cli_target(path));
                }
                // Unknown options are reported and rejected by `main` with exit 2.
                _ => {}
            }
        }

        // Never trigger single-instance forwarding side effects when unknown
        // options are present: `main` will reject the invocation with exit 2.
        let has_unknown = !Self::unknown_options(args).is_empty();
        let forwards = !has_unknown && !pre_dash.iter().any(|a| a == "--new-window");
        if forwards {
            if let StartDir::Path(path) = &start {
                if forward_probe(path) {
                    return Command::Forward { path: path.clone() };
                }
            } else if forward_probe(".") {
                return Command::Forward {
                    path: ".".to_string(),
                };
            }
        }

        Command::Window { view_mode, start }
    }

    /// Unknown options (before `--`). `main` logs them and exits 2 unless
    /// `--help` or `--version` was requested.
    pub fn unknown_options(args: &[String]) -> Vec<&str> {
        let mut unknown = Vec::new();
        let mut iter = args.iter();
        while let Some(arg) = iter.next() {
            let a = arg.as_str();
            if a == "--" {
                break;
            }
            if VALUE_FLAGS.contains(&a) {
                let _ = iter.next();
                continue;
            }
            if !a.starts_with('-') {
                continue;
            }
            let known = matches!(
                a,
                "-h" | "--help"
                    | "-V"
                    | "--version"
                    | "-0"
                    | "--null"
                    | "--new-window"
                    | "--diagnostics"
                    | "--grid"
                    | "--list"
                    | "--miller"
                    | "--chooser-prompt"
                    | "--prompter"
                    | "--choose-file"
                    | "--choose-files"
                    | "--choose-dir"
                    | "--save-file"
                    | "--title"
                    | "--name"
            );
            if !known {
                unknown.push(a);
            }
        }
        unknown
    }
}

/// Return the prefix of `args` before the first `--` end-of-options sentinel.
fn pre_double_dash(args: &[String]) -> &[String] {
    match args.iter().position(|a| a == "--") {
        Some(pos) => &args[..pos],
        None => args,
    }
}

/// Decode `file://` URIs passed via `.desktop` `%U` expansion into native paths;
/// plain filesystem paths pass through untouched.
fn normalize_cli_target(arg: &str) -> String {
    if arg.starts_with("file://")
        && let Some(path) = arca_engine::xdg::decode_file_uri(arg)
    {
        return path.to_string_lossy().into_owned();
    }
    arg.to_string()
}

fn parse_chooser(flag: &str, rest: &[String]) -> Option<ChooserInvocation> {
    use arca_engine::chooser::FileChooserMode;

    let (mode, multiple) = match flag {
        "--choose-file" => (FileChooserMode::OpenFile, false),
        "--choose-files" => (FileChooserMode::OpenFile, true),
        "--choose-dir" => (FileChooserMode::OpenDirectory, false),
        "--save-file" => (FileChooserMode::SaveFile, false),
        _ => return None,
    };

    let mut title = None;
    let mut name = None;
    let mut start_dir = None;
    let mut null_delimited = false;
    let mut after_double_dash = false;
    let mut iter = rest.iter();
    while let Some(arg) = iter.next() {
        if after_double_dash {
            start_dir = Some(normalize_cli_target(arg));
            continue;
        }
        match arg.as_str() {
            "--" => after_double_dash = true,
            "-0" | "--null" => null_delimited = true,
            "--title" => title = iter.next().cloned(),
            "--name" => name = iter.next().cloned(),
            dir if !dir.starts_with('-') => start_dir = Some(normalize_cli_target(dir)),
            // Unknown options are rejected in `main` with exit code 2.
            _ => {}
        }
    }

    Some(ChooserInvocation {
        mode,
        multiple,
        title,
        name,
        start_dir,
        null_delimited,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| s.to_string()).collect()
    }

    fn no_forward(_: &str) -> bool {
        false
    }

    #[test]
    fn help_and_version_win_over_everything_else() {
        assert_eq!(Command::parse(&args(&["-h"]), no_forward), Command::Help);
        assert_eq!(
            Command::parse(&args(&["--help", "/tmp"]), no_forward),
            Command::Help
        );
        assert_eq!(Command::parse(&args(&["-V"]), no_forward), Command::Version);
        assert_eq!(
            Command::parse(&args(&["--miller", "--version"]), no_forward),
            Command::Version
        );
    }

    #[test]
    fn prompter_mode_is_detected_under_both_spellings() {
        for flag in ["--chooser-prompt", "--prompter"] {
            assert_eq!(
                Command::parse(&args(&[flag]), no_forward),
                Command::Prompter
            );
            // Even alongside other arguments.
            assert_eq!(
                Command::parse(&args(&["/tmp", flag]), no_forward),
                Command::Prompter
            );
        }
    }

    #[test]
    fn every_picker_mode_maps_to_its_chooser_mode() {
        use arca_engine::chooser::FileChooserMode;

        let cases = [
            ("--choose-file", FileChooserMode::OpenFile, false),
            ("--choose-files", FileChooserMode::OpenFile, true),
            ("--choose-dir", FileChooserMode::OpenDirectory, false),
            ("--save-file", FileChooserMode::SaveFile, false),
        ];
        for (flag, mode, multiple) in cases {
            match Command::parse(&args(&[flag]), no_forward) {
                Command::Chooser(invocation) => {
                    assert_eq!(invocation.mode, mode, "{flag}");
                    assert_eq!(invocation.multiple, multiple, "{flag}");
                }
                other => panic!("{flag} produced {other:?}"),
            }
        }
    }

    #[test]
    fn picker_options_are_parsed_and_not_mistaken_for_directories() {
        let parsed = Command::parse(
            &args(&["--choose-file", "--title", "Pick me", "/tmp/docs"]),
            no_forward,
        );
        match parsed {
            Command::Chooser(invocation) => {
                assert_eq!(invocation.title.as_deref(), Some("Pick me"));
                assert_eq!(invocation.start_dir.as_deref(), Some("/tmp/docs"));
                assert_eq!(invocation.name, None);
            }
            other => panic!("expected a chooser, got {other:?}"),
        }

        let save = Command::parse(
            &args(&["--save-file", "--name", "report.pdf", "--title", "Save"]),
            no_forward,
        );
        match save {
            Command::Chooser(invocation) => {
                assert_eq!(invocation.name.as_deref(), Some("report.pdf"));
                assert_eq!(invocation.title.as_deref(), Some("Save"));
                assert_eq!(invocation.start_dir, None, "--name is a value, not a dir");
            }
            other => panic!("expected a chooser, got {other:?}"),
        }
    }

    #[test]
    fn view_mode_flags_select_the_mode() {
        for (flag, mode) in [
            ("--grid", ViewMode::Grid),
            ("--list", ViewMode::List),
            ("--miller", ViewMode::Miller),
        ] {
            match Command::parse(&args(&[flag]), no_forward) {
                Command::Window { view_mode, .. } => assert_eq!(view_mode, Some(mode), "{flag}"),
                other => panic!("{flag} produced {other:?}"),
            }
        }
        // The last mode flag wins.
        match Command::parse(&args(&["--grid", "--miller"]), no_forward) {
            Command::Window { view_mode, .. } => assert_eq!(view_mode, Some(ViewMode::Miller)),
            other => panic!("got {other:?}"),
        }
    }

    #[test]
    fn a_positional_argument_is_the_start_directory() {
        match Command::parse(&args(&["--miller", "/tmp/docs"]), no_forward) {
            Command::Window { view_mode, start } => {
                assert_eq!(view_mode, Some(ViewMode::Miller));
                assert_eq!(start, StartDir::Path("/tmp/docs".into()));
            }
            other => panic!("got {other:?}"),
        }
        match Command::parse(&args(&[]), no_forward) {
            Command::Window { start, .. } => assert_eq!(start, StartDir::WorkingDirectory),
            other => panic!("got {other:?}"),
        }
    }

    #[test]
    fn a_running_instance_takes_the_path_when_forwarding_is_allowed() {
        let accepted = |path: &str| path == "/tmp/docs";
        assert_eq!(
            Command::parse(&args(&["/tmp/docs"]), accepted),
            Command::Forward {
                path: "/tmp/docs".into()
            }
        );
        // With no path, the working directory is forwarded.
        assert_eq!(
            Command::parse(&args(&["--grid"]), |p| p == "."),
            Command::Forward { path: ".".into() }
        );
    }

    #[test]
    fn new_window_suppresses_forwarding() {
        let always = |_: &str| true;
        match Command::parse(&args(&["--new-window", "/tmp/docs"]), always) {
            Command::Window { start, .. } => {
                assert_eq!(start, StartDir::Path("/tmp/docs".into()))
            }
            other => panic!("--new-window must open a window, got {other:?}"),
        }
    }

    #[test]
    fn diagnostics_never_opens_a_window_or_forwards() {
        let forwarded = std::cell::Cell::new(false);
        let probe = |_: &str| {
            forwarded.set(true);
            true
        };
        assert_eq!(
            Command::parse(&args(&["--diagnostics", "/tmp"]), probe),
            Command::Diagnostics
        );
        assert!(!forwarded.get(), "the probe is never consulted");
        assert!(Command::unknown_options(&args(&["--diagnostics"])).is_empty());
        assert_eq!(
            Command::parse(&args(&["--diagnostics", "--help"]), no_forward),
            Command::Help,
            "help still wins"
        );
    }

    #[test]
    fn help_and_version_never_forward() {
        // A probe that accepts everything must not swallow --help.
        let always = |_: &str| true;
        assert_eq!(Command::parse(&args(&["--help"]), always), Command::Help);
        assert_eq!(Command::parse(&args(&["-V"]), always), Command::Version);
    }

    /// `--help` is the only place most users meet the CLI, and it drifted
    /// from `docs/reference/cli.md` before. Every flag the parser accepts
    /// must be named in the usage text.
    #[test]
    fn usage_text_names_every_accepted_flag() {
        for flag in [
            "--new-window",
            "--diagnostics",
            "--grid",
            "--list",
            "--miller",
            "--chooser-prompt",
            "--choose-file",
            "--choose-files",
            "--choose-dir",
            "--save-file",
            "--title",
            "--name",
            "-h",
            "-V",
        ] {
            assert!(USAGE.contains(flag), "usage text omits {flag}");
        }
    }

    /// Both spellings of a "stop and print" flag must win over the directory
    /// argument, and neither may be treated as a path even when it appears
    /// after one.
    #[test]
    fn help_and_version_win_over_a_directory_argument() {
        assert_eq!(
            Command::parse(&args(&["/tmp", "--help"]), no_forward),
            Command::Help
        );
        assert_eq!(
            Command::parse(&args(&["/tmp", "-V"]), no_forward),
            Command::Version
        );
        // A picker flag in a later position is not a mode selector: only the
        // first argument may start a chooser invocation.
        match Command::parse(&args(&["--grid", "--choose-file"]), no_forward) {
            Command::Window { .. } => {}
            other => panic!("a trailing picker flag must not switch mode: {other:?}"),
        }
    }

    /// A flag's value must never be captured as the start directory, in any
    /// order, and the last real positional argument wins.
    #[test]
    fn flag_values_are_never_mistaken_for_directories() {
        let parsed = Command::parse(
            &args(&["--save-file", "--title", "/not/a/dir", "/real"]),
            no_forward,
        );
        match parsed {
            Command::Chooser(invocation) => {
                assert_eq!(invocation.title.as_deref(), Some("/not/a/dir"));
                assert_eq!(invocation.start_dir.as_deref(), Some("/real"));
            }
            other => panic!("got {other:?}"),
        }

        match Command::parse(&args(&["/first", "/second"]), no_forward) {
            Command::Window { start, .. } => {
                assert_eq!(start, StartDir::Path("/second".into()), "last wins")
            }
            other => panic!("got {other:?}"),
        }
    }

    /// The forwarded path must be the one the window would have opened.
    #[test]
    fn the_forwarded_path_matches_the_window_path() {
        let seen = std::cell::RefCell::new(Vec::new());
        let probe = |path: &str| {
            seen.borrow_mut().push(path.to_string());
            false
        };
        Command::parse(&args(&["--miller", "/tmp/docs"]), probe);
        assert_eq!(*seen.borrow(), vec!["/tmp/docs".to_string()]);
    }

    #[test]
    fn unknown_options_are_collected_for_diagnostics() {
        let list = args(&["--grid", "--bogus", "/tmp", "-x"]);
        assert_eq!(Command::unknown_options(&list), vec!["--bogus", "-x"]);
        assert!(Command::unknown_options(&args(&["--grid", "/tmp"])).is_empty());
    }

    /// `.desktop` files declare `Exec=arca %U` and `x-scheme-handler/file`, so
    /// `OpenURI` invokes `arca` with `file://` URIs. Both window launch and
    /// single-instance forwarding must decode them into filesystem paths.
    #[test]
    fn file_uris_are_decoded_for_window_and_forwarding() {
        match Command::parse(&args(&["file:///tmp/My%20Docs"]), no_forward) {
            Command::Window { start, .. } => {
                assert_eq!(start, StartDir::Path("/tmp/My Docs".into()));
            }
            other => panic!("expected window with decoded path, got {other:?}"),
        }

        let seen = std::cell::RefCell::new(Vec::new());
        let probe = |path: &str| {
            seen.borrow_mut().push(path.to_string());
            true
        };
        assert_eq!(
            Command::parse(&args(&["file://localhost/tmp/dir%201"]), probe),
            Command::Forward {
                path: "/tmp/dir 1".into()
            }
        );
        assert_eq!(*seen.borrow(), vec!["/tmp/dir 1".to_string()]);
    }

    #[test]
    fn double_dash_and_null_delimiter_flags_are_parsed() {
        // `--` lets a directory starting with `-` be opened without being
        // misclassified as an unknown option.
        let dash_args = args(&["--", "-weird-dir"]);
        assert!(Command::unknown_options(&dash_args).is_empty());
        match Command::parse(&dash_args, no_forward) {
            Command::Window { start, .. } => {
                assert_eq!(start, StartDir::Path("-weird-dir".into()));
            }
            other => panic!("expected window for `-weird-dir`, got {other:?}"),
        }

        // `-0` and `--null` toggle NUL-delimited output on standalone pickers.
        for flag in ["-0", "--null"] {
            let parsed = Command::parse(&args(&["--choose-files", flag, "/tmp"]), no_forward);
            match parsed {
                Command::Chooser(inv) => {
                    assert!(inv.null_delimited);
                    assert_eq!(inv.start_dir.as_deref(), Some("/tmp"));
                }
                other => panic!("expected chooser, got {other:?}"),
            }
        }

        // `--` shields flag-like directory names even from `--help`/`-V`/`--diagnostics`.
        for name in ["--help", "-h", "-V", "--diagnostics", "--chooser-prompt"] {
            match Command::parse(&args(&["--", name]), no_forward) {
                Command::Window { start, .. } => {
                    assert_eq!(start, StartDir::Path(name.into()), "for {name}");
                }
                other => panic!("`-- {name}` must open directory, got {other:?}"),
            }
        }
    }

    #[test]
    fn unknown_options_never_trigger_single_instance_forwarding() {
        let forwarded = std::cell::Cell::new(false);
        let probe = |_: &str| {
            forwarded.set(true);
            true
        };
        let parsed = Command::parse(&args(&["--bogus", "/tmp"]), probe);
        assert!(
            !forwarded.get(),
            "invalid CLI invocations must never forward to a running instance"
        );
        assert!(matches!(parsed, Command::Window { .. }));

        // Value flags in window mode consume their value rather than treating it as a directory.
        match Command::parse(&args(&["--title", "NotADir"]), no_forward) {
            Command::Window { start, .. } => {
                assert_eq!(start, StartDir::WorkingDirectory);
            }
            other => panic!("expected working directory, got {other:?}"),
        }
    }
}
